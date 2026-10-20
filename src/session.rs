//! Runtime wiring of audio, MIDI, bank and config: owns the device handles and the ring producers.
//!
//! The UI reads state through `Session` and never touches cpal/midir directly. Threading: ADR 0002.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};

use ringbuf::HeapProd;
use ringbuf::traits::Producer;

use crate::audio::{self, AudioLinks, AudioShared, AudioStream, Command, StreamParams};
use crate::cli::DeviceSelector;
use crate::config::{Config, Settings};
use crate::midi::{self, DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiEvent, MidiInputHandle, MidiPortInfo, MidiStatus};
use crate::synth::{RustySynthEngine, SoundFontBank, SynthError};

/// Something the status line should tell the user (translated by the UI, spec N-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
	NoAudioDevice(String),
	AudioFallback,
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

pub struct Session {
	pub config: Config,
	pub config_path: PathBuf,
	pub shared: Arc<AudioShared>,
	audio: Option<AudioStream>,
	commands: Option<HeapProd<Command>>,
	retired: Option<ringbuf::HeapCons<Box<dyn crate::synth::SynthEngine>>>,
	/// The MIDI → audio producer while no port is connected.
	spare_events: Option<HeapProd<MidiEvent>>,
	midi: Option<MidiInputHandle<MidiSink>>,
	pub midi_status: MidiStatus,
	pub midi_ports: Vec<MidiPortInfo>,
	pub midi_dropped: Arc<AtomicU32>,
	_hotplug: Option<HotplugWatcher>,
	hotplug_rx: Receiver<Vec<MidiPortInfo>>,
	pub bank: Option<LoadedBank>,
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
			audio: None,
			commands: None,
			retired: None,
			spare_events: None,
			midi: None,
			midi_status: MidiStatus::NoPort,
			midi_ports: Vec::new(),
			midi_dropped: Arc::new(AtomicU32::new(0)),
			_hotplug: None,
			hotplug_rx,
			bank: None,
			notice: None,
		};

		session.open_audio(
			settings.audio_out.as_ref(),
			StreamParams {
				buffer_frames: settings.buffer_frames,
				sample_rate: settings.sample_rate,
			},
		);

		match midi::list_input_ports() {
			Ok(ports) => session.midi_ports = ports,
			Err(error) => session.notice = Some(Notice::MidiError(error.to_string())),
		}
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

	fn open_audio(&mut self, selector: Option<&DeviceSelector>, params: StreamParams) {
		let selected = match audio::select_output_device(selector) {
			Ok(selected) => selected,
			Err(error) => {
				self.notice = Some(Notice::NoAudioDevice(error.to_string()));
				return;
			}
		};
		if selected.requested_missing {
			self.notice = Some(Notice::AudioFallback);
		}
		match audio::open_output_stream(&selected.device, params, &self.shared) {
			Ok((stream, AudioLinks { events, commands, retired })) => {
				self.audio = Some(stream);
				self.commands = Some(commands);
				self.retired = Some(retired);
				self.spare_events = Some(events);
			}
			Err(error) => self.notice = Some(Notice::NoAudioDevice(error.to_string())),
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
				self.midi = Some(handle);
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
		self.midi_status = MidiStatus::NoPort;
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
		if let Err(error) = self.install_engine(&bank) {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
			return;
		}
		tracing::info!(name = bank.name(), presets = bank.presets().len(), path = %path.display(), "SoundFont loaded");
		self.config.remember_bank(path);
		self.config.synth.sf2 = Some(path.to_owned());
		self.bank = Some(LoadedBank {
			path: path.to_owned(),
			bank,
		});
	}

	fn install_engine(&mut self, bank: &SoundFontBank) -> Result<(), SynthError> {
		let (Some(stream), Some(commands)) = (&self.audio, self.commands.as_mut()) else {
			return Ok(());
		};
		let engine = RustySynthEngine::new(bank, stream.sample_rate)?;
		if commands.try_push(Command::ReplaceEngine(Some(Box::new(engine)))).is_err() {
			tracing::error!("command ring full, engine not installed");
		}
		Ok(())
	}

	pub fn audio(&self) -> Option<&AudioStream> {
		self.audio.as_ref()
	}

	/// Called once per UI tick: hot-plug updates and disposal of retired engines (off the audio thread).
	pub fn poll(&mut self) {
		while let Ok(ports) = self.hotplug_rx.try_recv() {
			let connected = self.midi.as_ref().map(|handle| handle.port_name.clone());
			self.midi_status = MidiStatus::from_ports(connected.as_deref(), &ports);
			tracing::info!(status = ?self.midi_status, ports = ports.len(), "MIDI ports changed");
			self.midi_ports = ports;
		}
		if let Some(retired) = self.retired.as_mut() {
			while let Some(engine) = ringbuf::traits::Consumer::try_pop(retired) {
				drop(engine);
			}
		}
	}
}
