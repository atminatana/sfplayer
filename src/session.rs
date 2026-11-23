//! Runtime wiring of audio, MIDI, bank and config: owns the device handles, the ring producers and the
//! thread that plays MIDI files.
//! Request handling (mixer, effects, presets, panic, notes) lives in `control.rs`, the audio stream and
//! the MIDI port in `devices.rs`, takes in `recording.rs`.
//!
//! The UI reads state through `Session` and never touches cpal/midir directly. Threading: ADR 0002.

mod control;
mod devices;
mod recording;

pub use control::{EffectsAction, EffectsSettings, MixerAction};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc::{self, Receiver, Sender};

use ringbuf::traits::{Consumer, Producer};
use ringbuf::{HeapCons, HeapProd};

use crate::audio::{self, AudioShared, AudioStream, Command, Mixer, RecordSource, StreamParams};
use crate::config::{Config, EngineKind, Settings};
use crate::midi::player::Player;
use crate::midi::smf::Song;
use crate::midi::{DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiEvent, MidiInputHandle, MidiPortInfo, MidiStatus};
use crate::record::{Recorder, Take};
use crate::synth::{SoundFontBank, SynthEngine, SynthError, Timbre};

/// Something the status line should tell the user (translated by the UI, spec N-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
	NoAudioDevice(String),
	AudioFallback,
	/// The chosen output did not open and another one plays in its place (spec v1.4, E-06); carries
	/// the error, which names the output.
	AudioFellBack(String),
	AudioReopened(String),
	AudioError(String),
	MidiFallback,
	MidiError(String),
	Sf2Error(String),
	ConfigError(String),
	RecordSaved(String),
	/// The take was cancelled and its file removed (spec v1.2, F-23).
	RecordCancelled,
	RecordError(String),
	/// The configured window font could not be loaded; carries its name (spec v1.2 N-14).
	FontFallback(String),
}

/// The MIDI file chosen to play, as the screen shows it (spec v1.2 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiFile {
	/// No file has been chosen.
	None,
	Loaded(PathBuf),
	/// The file remembered in the config is not there any more.
	Missing(PathBuf),
	/// The file is there and cannot be played; `reason` says why.
	Broken {
		path: PathBuf,
		reason: String,
	},
}

pub struct LoadedBank {
	pub path: PathBuf,
	pub bank: SoundFontBank,
}

/// State handed to the midir callback: the MIDI → audio ring producer plus its overflow counter.
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
	/// The recorder's ends of the recording rings while no take is running.
	record_rx: Option<RecordSource>,
	recorder: Option<Recorder>,
	/// The last take saved in this session.
	pub last_take: Option<Take>,
	/// The MIDI → audio producer while no port is connected.
	spare_events: Option<HeapProd<MidiEvent>>,
	midi: Option<MidiInputHandle<MidiSink>>,
	midi_port_index: Option<usize>,
	pub midi_status: MidiStatus,
	pub midi_ports: Vec<MidiPortInfo>,
	pub midi_dropped: Arc<AtomicU32>,
	/// The thread that plays MIDI files, once there is an audio stream to play into.
	player: Option<Player>,
	/// The player's count of stops the session has already cleaned up after.
	handled_stops: u32,
	pub midi_file: MidiFile,
	/// The chosen file, parsed: kept so that a player started later can be given it.
	song: Option<Arc<Song>>,
	_hotplug: Option<HotplugWatcher>,
	hotplug_rx: Receiver<Vec<MidiPortInfo>>,
	pub bank: Option<LoadedBank>,
	/// The timbre of the built-in synthesizer, shared with its engine on the audio thread.
	timbre: Arc<Timbre>,
	/// Preset chosen with F5 for every channel except percussion (spec v1.1 §4.1, ADR 0011);
	/// `None` = the engine default. Mirrors `config.synth.preset` while the bank it was chosen for is loaded.
	preset: Option<(u16, u8)>,
	pub mixer: Mixer,
	/// Semitones, −12..=12 (F-15).
	pub transpose: i8,
	pub notice: Option<Notice>,
}

impl Session {
	/// The session before any device is opened: no audio stream, no MIDI port, nothing loaded. The
	/// sender is the one the hot-plug watcher reports new port lists through.
	fn idle(settings: &Settings, config: Config, config_path: PathBuf) -> (Self, Sender<Vec<MidiPortInfo>>) {
		let master_percent = config.mixer.master.min(100);
		let shared = Arc::new(AudioShared::new(f32::from(master_percent) / 100.0));
		let (hotplug_tx, hotplug_rx) = mpsc::channel();
		let timbre = Arc::new(Timbre::new(config.synth.builtin.wave, config.synth.builtin.adsr()));
		let session = Self {
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
			record_rx: None,
			recorder: None,
			last_take: None,
			spare_events: None,
			midi: None,
			midi_port_index: None,
			midi_status: MidiStatus::NoPort,
			midi_ports: Vec::new(),
			midi_dropped: Arc::new(AtomicU32::new(0)),
			player: None,
			handled_stops: 0,
			midi_file: MidiFile::None,
			song: None,
			_hotplug: None,
			hotplug_rx,
			bank: None,
			timbre,
			preset: None,
			mixer: Mixer::new(master_percent),
			transpose: 0,
			notice: None,
		};
		(session, hotplug_tx)
	}

	/// A session with no device opened, for the tests of the layers above it.
	#[cfg(test)]
	pub(crate) fn detached(config: Config, config_path: PathBuf) -> Self {
		let settings = Settings {
			sf2: None,
			midi_in: None,
			audio_out: None,
			buffer_frames: crate::cli::DEFAULT_BUFFER_FRAMES,
			sample_rate: None,
		};
		Self::idle(&settings, config, config_path).0
	}

	/// Opens audio (silent until a bank is loaded), connects MIDI, loads the bank from settings, starts hot-plug polling.
	/// Device problems become notices, not errors: the UI must come up so the user can fix the choice.
	pub fn start(settings: &Settings, config: Config, config_path: PathBuf) -> Self {
		let (mut session, hotplug_tx) = Self::idle(settings, config, config_path);
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

		// A bank that fails to load is already a notice; the UI comes up so the user can pick another
		// file with F4. Loading a bank installs the engine; without one the built-in synthesizer still
		// has to be installed, if it is the chosen processor — it needs no bank (spec v1.2, F-22).
		let is_bank_loaded = settings.sf2.clone().is_some_and(|path| session.load_bank(&path).is_ok());
		if !is_bank_loaded && let Err(error) = session.install_engine() {
			session.notice = Some(Notice::Sf2Error(error.to_string()));
		}
		// The MIDI file of the last session is chosen again, not played; if it is gone or broken,
		// its row says so.
		if let Some(path) = session.config.source.midi_file.clone()
			&& let Err(error) = session.load_midi_file(&path)
		{
			session.midi_file = control::midi_file_problem(&path, &error);
		}
		session
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
		// The remembered preset belongs to the remembered bank (F-14); another file starts from its first preset.
		let same_bank = self.config.synth.sf2.as_deref() == Some(path);
		self.preset = self.config.synth.preset.filter(|_| same_bank).filter(|(bank_number, program)| {
			bank.presets()
				.iter()
				.any(|preset| (preset.bank, preset.program) == (*bank_number, *program))
		});
		self.config.synth.preset = self.preset;
		self.config.remember_bank(path);
		self.config.synth.sf2 = Some(path.to_owned());
		self.bank = Some(LoadedBank {
			path: path.to_owned(),
			bank,
		});
		self.notice = None;
		if let Err(error) = self.install_engine() {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
			return Err(error);
		}
		Ok(())
	}

	/// Creates the engine of the chosen processor at the stream's sample rate and swaps it in; re-applies
	/// the preset (a SoundFont has presets, the built-in synthesizer has none) and the effect sends.
	fn install_engine(&mut self) -> Result<(), SynthError> {
		let (Some(stream), Some(commands)) = (&self.audio, self.commands.as_mut()) else {
			return Ok(());
		};
		let kind = self.config.synth.engine;
		let bank = self.bank.as_ref().map(|loaded| &loaded.bank);
		let effects = self.config.synth.reverb || self.config.synth.chorus;
		let engine = control::build_engine(kind, bank, &self.timbre, stream.sample_rate, effects)?;
		if commands.try_push(Command::ReplaceEngine(engine)).is_err() {
			tracing::error!("command ring full, engine not installed");
			return Ok(());
		}
		if kind == EngineKind::Sf2
			&& let Some((bank, program)) = self.preset
		{
			control::push_global_preset(commands, bank, program);
		}
		control::push_effect_sends(commands, &self.config.synth);
		Ok(())
	}

	/// F-14: writes devices, bank, volumes, effects and format to the config file; failures become a notice.
	pub fn save_config(&mut self) {
		if let Err(error) = self.config.save(&self.config_path) {
			tracing::warn!(%error, path = %self.config_path.display(), "config save failed");
			self.notice = Some(Notice::ConfigError(error.to_string()));
		} else {
			tracing::info!(path = %self.config_path.display(), "config saved");
		}
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
		self.clean_up_after_playback();
	}

	/// When a MIDI file has played to its end or was stopped, puts back what it changed in the
	/// synthesizer (F-21). The player counts its stops; each is cleaned up after once.
	fn clean_up_after_playback(&mut self) {
		let Some(stops) = self.player.as_ref().map(Player::stops) else {
			return;
		};
		if stops == self.handled_stops {
			return;
		}
		self.handled_stops = stops;
		if let Some(commands) = self.commands.as_mut() {
			control::push_after_playback(commands, self.preset, &self.config.synth);
		}
	}
}

#[cfg(test)]
mod tests;
