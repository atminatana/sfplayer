//! Runtime wiring of audio, MIDI, bank and config: owns the device handles and the ring producers.
//!
//! The UI reads state through `Session` and never touches cpal/midir directly. Threading: ADR 0002.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};

use ringbuf::traits::{Consumer, Producer};
use ringbuf::{HeapCons, HeapProd};

use crate::audio::{self, AudioLinks, AudioShared, AudioStream, Command, StreamParams};
use crate::cli::DeviceSelector;
use crate::config::{Config, RecordFormat, Settings};
use crate::midi::{self, DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiEvent, MidiInputHandle, MidiPortInfo, MidiStatus, controller};
use crate::synth::{PresetInfo, RustySynthEngine, SoundFontBank, SynthEngine, SynthError};

/// Something the status line should tell the user (translated by the UI, spec N-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
	NoAudioDevice(String),
	AudioFallback,
	AudioReopened(String),
	AudioError(String),
	MidiFallback,
	MidiError(String),
	Sf2Error(String),
}

pub struct LoadedBank {
	pub path: PathBuf,
	pub bank: SoundFontBank,
}

/// State handed to the midir callback: the ring producer plus an overflow counter.
pub struct MidiSink {
	events: HeapProd<MidiEvent>,
	dropped: Arc<AtomicU32>,
}

pub const MIDI_CHANNELS: usize = 16;

pub struct Session {
	pub config: Config,
	pub config_path: PathBuf,
	pub shared: Arc<AudioShared>,
	params: StreamParams,
	audio: Option<AudioStream>,
	commands: Option<HeapProd<Command>>,
	retired: Option<HeapCons<Box<dyn SynthEngine>>>,
	/// The MIDI → audio producer while no port is connected.
	spare_events: Option<HeapProd<MidiEvent>>,
	midi: Option<MidiInputHandle<MidiSink>>,
	midi_port_index: Option<usize>,
	pub midi_status: MidiStatus,
	pub midi_ports: Vec<MidiPortInfo>,
	pub midi_dropped: Arc<AtomicU32>,
	_hotplug: Option<HotplugWatcher>,
	hotplug_rx: Receiver<Vec<MidiPortInfo>>,
	pub bank: Option<LoadedBank>,
	/// Preset chosen per MIDI channel (F-06); `None` = the engine default.
	channel_presets: [Option<(u16, u8)>; MIDI_CHANNELS],
	/// Channel the mixer and the preset dialog act on (0-based).
	pub current_channel: u8,
	pub notice: Option<Notice>,
}

impl Session {
	/// Opens audio (silent until a bank is loaded), connects MIDI, loads the bank from settings, starts hot-plug polling.
	/// Device problems become notices, not errors: the UI must come up so the user can fix the choice.
	pub fn start(settings: &Settings, config: Config, config_path: PathBuf) -> Self {
		let shared = Arc::new(AudioShared::new(f32::from(config.mixer.master) / 100.0));
		let (hotplug_tx, hotplug_rx) = mpsc::channel();
		let mut session = Self {
			config,
			config_path,
			shared,
			params: StreamParams {
				buffer_frames: settings.buffer_frames,
				sample_rate: settings.sample_rate,
			},
			audio: None,
			commands: None,
			retired: None,
			spare_events: None,
			midi: None,
			midi_port_index: None,
			midi_status: MidiStatus::NoPort,
			midi_ports: Vec::new(),
			midi_dropped: Arc::new(AtomicU32::new(0)),
			_hotplug: None,
			hotplug_rx,
			bank: None,
			channel_presets: [None; MIDI_CHANNELS],
			current_channel: 0,
			notice: None,
		};

		let selected = match audio::select_output_device(settings.audio_out.as_ref()) {
			Ok(selected) => Some(selected),
			Err(error) => {
				session.notice = Some(Notice::NoAudioDevice(error.to_string()));
				None
			}
		};
		if let Some(selected) = selected {
			if selected.requested_missing {
				session.notice = Some(Notice::AudioFallback);
			}
			session.open_audio(&selected.device);
		}

		session.rescan();
		if let Some(index) = session.pick_midi_port(settings.midi_in.as_ref()) {
			session.connect_midi(index);
		}
		session._hotplug = Some(HotplugWatcher::spawn(
			DEFAULT_POLL_INTERVAL,
			session.midi_ports.clone(),
			move |ports| {
				let _ = hotplug_tx.send(ports);
			},
		));

		if let Some(path) = &settings.sf2 {
			let path = path.clone();
			session.load_bank(&path);
		}
		session
	}

	fn open_audio(&mut self, device: &cpal::Device) {
		match audio::open_output_stream(device, self.params, &self.shared) {
			Ok((stream, AudioLinks { events, commands, retired })) => {
				self.audio = Some(stream);
				self.commands = Some(commands);
				self.retired = Some(retired);
				self.spare_events = Some(events);
			}
			Err(error) => self.notice = Some(Notice::NoAudioDevice(error.to_string())),
		}
	}

	/// Switches to another output device (F-03): new stream and rings, engine re-created for the new rate,
	/// MIDI reconnected to the new ring, channel presets re-applied.
	pub fn reopen_audio(&mut self, index: usize) {
		let device = match audio::output_device_by_index(index) {
			Ok(device) => device,
			Err(error) => {
				self.notice = Some(Notice::AudioError(error.to_string()));
				return;
			}
		};
		let midi_index = self.midi_port_index;
		self.disconnect_midi();
		self.audio = None;
		self.commands = None;
		self.retired = None;
		self.spare_events = None;
		self.open_audio(&device);
		let Some(stream) = &self.audio else {
			return;
		};
		let device_name = stream.device_name.clone();
		self.config.audio.output = Some(device_name.clone());
		self.notice = Some(Notice::AudioReopened(device_name));
		if let Err(error) = self.install_engine() {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
		}
		if let Some(index) = midi_index {
			self.connect_midi(index);
		}
	}

	/// Resolves the selector against the known ports: no match → first port with a notice; no ports → `None`.
	fn pick_midi_port(&mut self, selector: Option<&DeviceSelector>) -> Option<usize> {
		let first = self.midi_ports.first().map(|port| port.index);
		let selector = selector?;
		match selector.find(self.midi_ports.iter().map(|port| port.name.as_str())) {
			Some(index) => Some(index),
			None => {
				tracing::warn!(?selector, "MIDI input not found, using the first available port");
				self.notice = Some(Notice::MidiFallback);
				first
			}
		}
	}

	/// Re-enumerates MIDI ports (F8) and refreshes the connection status.
	pub fn rescan(&mut self) {
		match midi::list_input_ports() {
			Ok(ports) => {
				self.midi_ports = ports;
				self.refresh_midi_status();
			}
			Err(error) => self.notice = Some(Notice::MidiError(error.to_string())),
		}
	}

	fn refresh_midi_status(&mut self) {
		let connected = self.midi.as_ref().map(|handle| handle.port_name.clone());
		self.midi_status = MidiStatus::from_ports(connected.as_deref(), &self.midi_ports);
	}

	/// Connects to the MIDI port, moving the event producer into the midir callback.
	pub fn connect_midi(&mut self, index: usize) {
		self.disconnect_midi();
		let Some(events) = self.spare_events.take() else {
			return;
		};
		let sink = MidiSink {
			events,
			dropped: Arc::clone(&self.midi_dropped),
		};
		match midi::connect_input(index, sink, |event, sink| {
			if sink.events.try_push(event).is_err() {
				sink.dropped.fetch_add(1, Ordering::Relaxed);
			}
		}) {
			Ok(handle) => {
				self.midi_status = MidiStatus::Connected(handle.port_name.clone());
				self.config.midi.input = Some(handle.port_name.clone());
				self.midi = Some(handle);
				self.midi_port_index = Some(index);
			}
			Err(error) => {
				tracing::warn!(%error, index, "MIDI connect failed");
				self.notice = Some(Notice::MidiError(error.to_string()));
			}
		}
	}

	pub fn disconnect_midi(&mut self) {
		if let Some(handle) = self.midi.take() {
			self.spare_events = Some(handle.close().events);
		}
		self.midi_port_index = None;
		self.midi_status = MidiStatus::NoPort;
	}

	pub fn connected_midi_port(&self) -> Option<&str> {
		self.midi.as_ref().map(|handle| handle.port_name.as_str())
	}

	/// Loads a bank and installs a matching engine on the audio thread (F-05); failures become a notice.
	pub fn load_bank(&mut self, path: &Path) {
		let bank = match SoundFontBank::load(path) {
			Ok(bank) => bank,
			Err(error) => {
				tracing::warn!(%error, path = %path.display(), "SoundFont load failed");
				self.notice = Some(Notice::Sf2Error(error.to_string()));
				return;
			}
		};
		tracing::info!(name = bank.name(), presets = bank.presets().len(), path = %path.display(), "SoundFont loaded");
		self.config.remember_bank(path);
		self.config.synth.sf2 = Some(path.to_owned());
		self.bank = Some(LoadedBank {
			path: path.to_owned(),
			bank,
		});
		self.channel_presets = [None; MIDI_CHANNELS];
		if let Err(error) = self.install_engine() {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
		}
	}

	/// Creates an engine for the current bank at the stream's sample rate and swaps it in; re-applies presets.
	fn install_engine(&mut self) -> Result<(), SynthError> {
		let (Some(stream), Some(commands), Some(loaded)) = (&self.audio, self.commands.as_mut(), &self.bank) else {
			return Ok(());
		};
		let engine = RustySynthEngine::new(&loaded.bank, stream.sample_rate)?;
		if commands.try_push(Command::ReplaceEngine(Some(Box::new(engine)))).is_err() {
			tracing::error!("command ring full, engine not installed");
			return Ok(());
		}
		for channel in 0..MIDI_CHANNELS {
			if let Some((bank, program)) = self.channel_presets[channel] {
				push_preset(commands, channel as u8, bank, program);
			}
		}
		Ok(())
	}

	/// Chooses the preset for `channel` (F-06) by sending Bank Select + Program Change down the command ring.
	pub fn select_preset(&mut self, channel: u8, bank: u16, program: u8) {
		let Some(slot) = self.channel_presets.get_mut(usize::from(channel)) else {
			return;
		};
		*slot = Some((bank, program));
		if let Some(commands) = self.commands.as_mut() {
			push_preset(commands, channel, bank, program);
		}
	}

	pub fn preset_for(&self, channel: u8) -> Option<(u16, u8)> {
		self.channel_presets.get(usize::from(channel)).copied().flatten()
	}

	/// The preset shown for `channel`: the chosen one, or the engine default (first preset of the bank).
	pub fn preset_info(&self, channel: u8) -> Option<&PresetInfo> {
		let presets = self.bank.as_ref()?.bank.presets();
		match self.preset_for(channel) {
			Some((bank, program)) => presets.iter().find(|preset| (preset.bank, preset.program) == (bank, program)),
			None => presets.first(),
		}
	}

	pub fn set_record_format(&mut self, format: RecordFormat) {
		self.config.record.format = format;
	}

	pub fn audio(&self) -> Option<&AudioStream> {
		self.audio.as_ref()
	}

	/// Called once per UI tick: hot-plug updates and disposal of retired engines (off the audio thread).
	pub fn poll(&mut self) {
		let mut changed = false;
		while let Ok(ports) = self.hotplug_rx.try_recv() {
			self.midi_ports = ports;
			changed = true;
		}
		if changed {
			self.refresh_midi_status();
			tracing::info!(status = ?self.midi_status, ports = self.midi_ports.len(), "MIDI ports changed");
		}
		if let Some(retired) = self.retired.as_mut() {
			while let Some(engine) = retired.try_pop() {
				drop(engine);
			}
		}
	}
}

/// Bank Select (CC 0, 7-bit; the percussion bank 128 is implied by channel 10 in SF2) followed by Program Change.
fn push_preset(commands: &mut HeapProd<Command>, channel: u8, bank: u16, program: u8) {
	let bank_msb = u8::try_from(bank & 0x7F).unwrap_or(0);
	let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
		channel,
		controller: controller::BANK_SELECT,
		value: bank_msb,
	}));
	let _ = commands.try_push(Command::Midi(MidiEvent::ProgramChange { channel, program }));
}
