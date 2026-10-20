//! Runtime wiring of audio, MIDI, bank and config: owns the device handles and the ring producers.
//!
//! The UI reads state through `Session` and never touches cpal/midir directly. Threading: ADR 0002.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};

use ringbuf::traits::{Consumer, Producer};
use ringbuf::{HeapCons, HeapProd};

use crate::audio::mixer::{CHANNEL_STEP, MASTER_STEP, MASTER_STEP_FAST};
use crate::audio::{self, AudioLinks, AudioShared, AudioStream, Command, Mixer, StreamParams};
use crate::cli::DeviceSelector;
use crate::config::{Config, RecordFormat, Settings};
use crate::midi::{
	self, DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiEvent, MidiInputHandle, MidiPortInfo, MidiStatus, MonitorEntry, controller,
};
use crate::synth::{PresetInfo, RustySynthEngine, SoundFontBank, SynthEngine, SynthError};
use ringbuf::HeapRb;
use ringbuf::traits::Split;

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

/// State handed to the midir callback: both ring producers plus overflow counters.
pub struct MidiSink {
	events: HeapProd<MidiEvent>,
	monitor: HeapProd<MonitorEntry>,
	dropped: Arc<AtomicU32>,
	monitor_dropped: Arc<AtomicU32>,
}

/// Capacity of the MIDI → UI monitor ring; drained every UI frame (≤ 33 ms).
pub const MONITOR_RING_CAPACITY: usize = 1024;

pub const MIDI_CHANNELS: usize = 16;

/// Reverb / chorus switches (F-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectsAction {
	ToggleReverb,
	ToggleChorus,
}

/// What the mixer keys do (spec F-10, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerAction {
	MasterUp,
	MasterDown,
	MasterUpFast,
	MasterDownFast,
	MasterMute,
	/// 0-based channel.
	SelectChannel(u8),
	ChannelUp,
	ChannelDown,
	ChannelMute,
	ChannelSolo,
}

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
	/// The MIDI → UI monitor producer while no port is connected.
	spare_monitor: Option<HeapProd<MonitorEntry>>,
	monitor_rx: HeapCons<MonitorEntry>,
	monitor_dropped: Arc<AtomicU32>,
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
	pub mixer: Mixer,
	/// Semitones, −12..=12 (F-15).
	pub transpose: i8,
	pub notice: Option<Notice>,
}

impl Session {
	/// Opens audio (silent until a bank is loaded), connects MIDI, loads the bank from settings, starts hot-plug polling.
	/// Device problems become notices, not errors: the UI must come up so the user can fix the choice.
	pub fn start(settings: &Settings, config: Config, config_path: PathBuf) -> Self {
		let master_percent = config.mixer.master.min(100);
		let shared = Arc::new(AudioShared::new(f32::from(master_percent) / 100.0));
		let (hotplug_tx, hotplug_rx) = mpsc::channel();
		let (monitor_tx, monitor_rx) = HeapRb::<MonitorEntry>::new(MONITOR_RING_CAPACITY).split();
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
			spare_monitor: Some(monitor_tx),
			monitor_rx,
			monitor_dropped: Arc::new(AtomicU32::new(0)),
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
			mixer: Mixer::new(master_percent),
			transpose: 0,
			notice: None,
		};
		session.shared.set_omni_channel(
			session
				.config
				.midi
				.omni_channel
				.and_then(|channel| channel.checked_sub(1))
				.filter(|channel| *channel < 16),
		);

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
			// The failure is already a notice; the UI comes up so the user can pick another file with F4.
			let _ = session.load_bank(&path);
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
		// `spare_monitor` is kept: the monitor ring does not depend on the audio stream.
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
		let (Some(events), Some(monitor)) = (self.spare_events.take(), self.spare_monitor.take()) else {
			return;
		};
		let sink = MidiSink {
			events,
			monitor,
			dropped: Arc::clone(&self.midi_dropped),
			monitor_dropped: Arc::clone(&self.monitor_dropped),
		};
		match midi::connect_input(index, sink, |event, sink| {
			if sink.events.try_push(event).is_err() {
				sink.dropped.fetch_add(1, Ordering::Relaxed);
			}
			if sink.monitor.try_push(MonitorEntry::now(event)).is_err() {
				sink.monitor_dropped.fetch_add(1, Ordering::Relaxed);
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

	/// Monitor entries that arrived since the last call (MIDI thread → UI, F-08).
	pub fn drain_monitor(&mut self) -> impl Iterator<Item = MonitorEntry> + '_ {
		std::iter::from_fn(|| self.monitor_rx.try_pop())
	}

	pub fn monitor_dropped(&self) -> u32 {
		self.monitor_dropped.load(Ordering::Relaxed)
	}

	pub fn disconnect_midi(&mut self) {
		if let Some(handle) = self.midi.take() {
			let sink = handle.close();
			self.spare_events = Some(sink.events);
			self.spare_monitor = Some(sink.monitor);
		}
		self.midi_port_index = None;
		self.midi_status = MidiStatus::NoPort;
	}

	pub fn connected_midi_port(&self) -> Option<&str> {
		self.midi.as_ref().map(|handle| handle.port_name.as_str())
	}

	/// Loads a bank and installs a matching engine on the audio thread (F-05).
	/// Failures are returned for the dialog and also become a status notice.
	pub fn load_bank(&mut self, path: &Path) -> Result<(), SynthError> {
		let bank = match SoundFontBank::load(path) {
			Ok(bank) => bank,
			Err(error) => {
				tracing::warn!(%error, path = %path.display(), "SoundFont load failed");
				self.notice = Some(Notice::Sf2Error(error.to_string()));
				return Err(error);
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
		self.notice = None;
		if let Err(error) = self.install_engine() {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
			return Err(error);
		}
		Ok(())
	}

	/// Creates an engine for the current bank at the stream's sample rate and swaps it in; re-applies presets.
	fn install_engine(&mut self) -> Result<(), SynthError> {
		let (Some(stream), Some(commands), Some(loaded)) = (&self.audio, self.commands.as_mut(), &self.bank) else {
			return Ok(());
		};
		let effects = self.config.synth.reverb || self.config.synth.chorus;
		let engine = RustySynthEngine::new(&loaded.bank, stream.sample_rate, effects)?;
		if commands.try_push(Command::ReplaceEngine(Some(Box::new(engine)))).is_err() {
			tracing::error!("command ring full, engine not installed");
			return Ok(());
		}
		for channel in 0..MIDI_CHANNELS {
			if let Some((bank, program)) = self.channel_presets[channel] {
				push_preset(commands, channel as u8, bank, program);
			}
		}
		push_channel_volumes(commands, &self.mixer.effective_volumes());
		push_effect_sends(commands, &self.config.synth);
		Ok(())
	}

	/// Toggles an effect (F-13). Turning the last effect off or the first one on rebuilds the engine,
	/// because rustysynth decides at construction whether the effect units exist; otherwise only the sends change.
	pub fn apply_effects(&mut self, action: EffectsAction) {
		let had_effects = self.config.synth.reverb || self.config.synth.chorus;
		match action {
			EffectsAction::ToggleReverb => self.config.synth.reverb = !self.config.synth.reverb,
			EffectsAction::ToggleChorus => self.config.synth.chorus = !self.config.synth.chorus,
		}
		let has_effects = self.config.synth.reverb || self.config.synth.chorus;
		if has_effects != had_effects {
			if let Err(error) = self.install_engine() {
				self.notice = Some(Notice::Sf2Error(error.to_string()));
			}
		} else if let Some(commands) = self.commands.as_mut() {
			push_effect_sends(commands, &self.config.synth);
		}
	}

	/// Current send levels for the UI: `None` when the effect is off.
	pub fn reverb_level(&self) -> Option<u8> {
		self.config.synth.reverb.then_some(self.config.synth.reverb_level.min(127))
	}

	pub fn chorus_level(&self) -> Option<u8> {
		self.config.synth.chorus.then_some(self.config.synth.chorus_level.min(127))
	}

	/// Applies a mixer key (F-10): master through the gain atomic, channels through CC 7 on the command ring.
	pub fn apply_mixer(&mut self, action: MixerAction) {
		let channel = usize::from(self.current_channel);
		match action {
			MixerAction::MasterUp => self.mixer.adjust_master(MASTER_STEP),
			MixerAction::MasterDown => self.mixer.adjust_master(-MASTER_STEP),
			MixerAction::MasterUpFast => self.mixer.adjust_master(MASTER_STEP_FAST),
			MixerAction::MasterDownFast => self.mixer.adjust_master(-MASTER_STEP_FAST),
			MixerAction::MasterMute => self.mixer.toggle_master_mute(),
			MixerAction::SelectChannel(index) => {
				if usize::from(index) < MIDI_CHANNELS {
					self.current_channel = index;
				}
				return;
			}
			MixerAction::ChannelUp => self.mixer.adjust_channel(channel, CHANNEL_STEP),
			MixerAction::ChannelDown => self.mixer.adjust_channel(channel, -CHANNEL_STEP),
			MixerAction::ChannelMute => self.mixer.toggle_mute(channel),
			MixerAction::ChannelSolo => self.mixer.toggle_solo(channel),
		}
		self.shared.set_master_gain(self.mixer.master_gain());
		self.config.mixer.master = self.mixer.master;
		if let Some(commands) = self.commands.as_mut() {
			push_channel_volumes(commands, &self.mixer.effective_volumes());
		}
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

	/// `[` / `]`: transposition in semitones, clamped to ±12 (F-15).
	pub fn adjust_transpose(&mut self, delta: i8) {
		self.transpose = (i16::from(self.transpose) + i16::from(delta)).clamp(-12, 12) as i8;
		self.shared.set_transpose(self.transpose);
	}

	pub fn omni_channel(&self) -> Option<u8> {
		self.shared.omni_channel()
	}

	/// Notes from the text keyboard (F-16): same path as hardware MIDI, via the command ring (ADR 0009).
	pub fn send_notes(&mut self, events: &[MidiEvent]) {
		let Some(commands) = self.commands.as_mut() else {
			return;
		};
		for event in events {
			if commands.try_push(Command::Midi(*event)).is_err() {
				tracing::warn!("command ring full, text keyboard event dropped");
			}
		}
	}

	/// F-11: All Notes Off + All Sound Off on every channel.
	pub fn panic(&mut self) {
		let Some(commands) = self.commands.as_mut() else {
			return;
		};
		for channel in 0..MIDI_CHANNELS as u8 {
			let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
				channel,
				controller: controller::ALL_NOTES_OFF,
				value: 0,
			}));
			let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
				channel,
				controller: controller::ALL_SOUND_OFF,
				value: 0,
			}));
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

/// CC 91 / CC 93 for all channels: the configured level while the effect is on, 0 when off (F-13).
fn push_effect_sends(commands: &mut HeapProd<Command>, synth: &crate::config::SynthConfig) {
	let reverb = if synth.reverb { synth.reverb_level.min(127) } else { 0 };
	let chorus = if synth.chorus { synth.chorus_level.min(127) } else { 0 };
	for channel in 0..MIDI_CHANNELS as u8 {
		let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
			channel,
			controller: controller::REVERB_SEND,
			value: reverb,
		}));
		let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
			channel,
			controller: controller::CHORUS_SEND,
			value: chorus,
		}));
	}
}

/// CC 7 for all 16 channels; cheap enough to resend on every mixer change (16 ring pushes).
fn push_channel_volumes(commands: &mut HeapProd<Command>, volumes: &[u8; MIDI_CHANNELS]) {
	for (channel, volume) in volumes.iter().enumerate() {
		let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
			channel: channel as u8,
			controller: controller::VOLUME,
			value: *volume,
		}));
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
