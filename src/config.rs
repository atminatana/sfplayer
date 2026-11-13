//! TOML configuration (spec N-06) in the OS config directory; section layout for keys: ADR 0007.
//!
//! Every field has a default and unknown keys are ignored, so an old or partial file always loads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, DEFAULT_BUFFER_FRAMES, DeviceSelector, EmptyDeviceSelector};
use crate::synth::{Adsr, AdsrPreset, Wave};

pub const APP_NAME: &str = "sf2_player";
const CONFIG_FILE_NAME: &str = "config.toml";
/// How many folders for takes are remembered (spec v1.2 §4.2).
pub const RECENT_DIRS_LIMIT: usize = 10;
/// How many recently played MIDI files are remembered (spec v1.2 §7).
pub const RECENT_MIDI_FILES_LIMIT: usize = 10;
/// How many recently used banks are remembered (spec F-05).
pub const RECENT_BANKS_LIMIT: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	#[error("cannot read config file {path}")]
	Read {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("config file {path} is not valid TOML")]
	Parse {
		path: PathBuf,
		#[source]
		source: toml::de::Error,
	},
	#[error("cannot write config file {path}")]
	Write {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("cannot serialize config")]
	Serialize(#[from] toml::ser::Error),
	#[error("config key {key} is not a valid device selector")]
	DeviceSelector {
		key: &'static str,
		#[source]
		source: EmptyDeviceSelector,
	},
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
	pub midi: MidiConfig,
	/// The MIDI file source (spec v1.2, F-21).
	pub source: SourceConfig,
	pub audio: AudioConfig,
	pub synth: SynthConfig,
	pub mixer: MixerConfig,
	pub keyboard: KeyboardConfig,
	/// Command hotkey overrides: command name → key chord (parsed in the UI layer).
	pub keys: BTreeMap<String, String>,
	pub ui: UiConfig,
	pub record: RecordConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiConfig {
	/// Port name substring or index.
	pub input: Option<String>,
	/// 1-based channel all incoming notes are redirected to (F-15 omni); `None` = off.
	pub omni_channel: Option<u8>,
}

/// `[source]`: the MIDI file chosen to play and the ones chosen before it, most recent first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
	pub midi_file: Option<PathBuf>,
	pub recent_midi_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
	/// Device name substring or index.
	pub output: Option<String>,
	pub buffer_frames: u32,
	pub sample_rate: Option<u32>,
}

impl Default for AudioConfig {
	fn default() -> Self {
		Self {
			output: None,
			buffer_frames: DEFAULT_BUFFER_FRAMES,
			sample_rate: None,
		}
	}
}

/// The processor that turns notes into sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
	/// A SoundFont bank played by rustysynth.
	#[default]
	Sf2,
	/// The built-in synthesizer: needs no bank.
	Builtin,
}

impl EngineKind {
	pub fn other(self) -> Self {
		match self {
			Self::Sf2 => Self::Builtin,
			Self::Builtin => Self::Sf2,
		}
	}
}

/// One of the four values of the envelope, for the buttons that step it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdsrField {
	Attack,
	Decay,
	Sustain,
	Release,
}

/// `[synth.builtin]`: the wave and the envelope of the built-in synthesizer (spec v1.2 §7). Times in
/// seconds, the sustain level as a share of the peak.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuiltinConfig {
	pub wave: Wave,
	pub adsr_preset: AdsrPreset,
	pub attack: f32,
	pub decay: f32,
	pub sustain: f32,
	pub release: f32,
}

impl Default for BuiltinConfig {
	fn default() -> Self {
		let mut config = Self {
			wave: Wave::default(),
			adsr_preset: AdsrPreset::default(),
			attack: 0.0,
			decay: 0.0,
			sustain: 0.0,
			release: 0.0,
		};
		config.apply_preset(AdsrPreset::default());
		config
	}
}

impl BuiltinConfig {
	/// A step of the three times, in seconds, and of the sustain level (spec v1.2, F-22).
	pub const TIME_STEP: f32 = 0.005;
	pub const SUSTAIN_STEP: f32 = 0.05;

	/// The envelope as the engine takes it: every value inside its limits.
	pub fn adsr(&self) -> Adsr {
		Adsr {
			attack: self.attack,
			decay: self.decay,
			sustain: self.sustain,
			release: self.release,
		}
		.clamped()
	}

	/// Takes the values of a ready-made preset; choosing `Custom` keeps the values as they are.
	pub fn apply_preset(&mut self, preset: AdsrPreset) {
		self.adsr_preset = preset;
		if let Some(adsr) = preset.adsr() {
			self.set_adsr(adsr);
		}
	}

	/// Moves one value by `steps` steps, inside its limits. The envelope is then the user's own:
	/// no preset is chosen any more.
	pub fn adjust(&mut self, field: AdsrField, steps: i8) {
		let mut adsr = self.adsr();
		let (value, step) = match field {
			AdsrField::Attack => (&mut adsr.attack, Self::TIME_STEP),
			AdsrField::Decay => (&mut adsr.decay, Self::TIME_STEP),
			AdsrField::Sustain => (&mut adsr.sustain, Self::SUSTAIN_STEP),
			AdsrField::Release => (&mut adsr.release, Self::TIME_STEP),
		};
		*value += step * f32::from(steps);
		self.set_adsr(adsr.clamped());
		self.adsr_preset = AdsrPreset::Custom;
	}

	/// Stores the envelope rounded to what the screen shows — milliseconds and whole per cent — so
	/// that steps do not pile up the errors of binary fractions.
	fn set_adsr(&mut self, adsr: Adsr) {
		let rounded = |value: f32, places: f32| (value * places).round() / places;
		self.attack = rounded(adsr.attack, 1000.0);
		self.decay = rounded(adsr.decay, 1000.0);
		self.sustain = rounded(adsr.sustain, 100.0);
		self.release = rounded(adsr.release, 1000.0);
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthConfig {
	pub sf2: Option<PathBuf>,
	/// Most recent first, at most `RECENT_BANKS_LIMIT` entries.
	pub recent_banks: Vec<PathBuf>,
	/// `[bank, program]` chosen with F5 for every channel except 10 (spec v1.1 §4.1, ADR 0011);
	/// `None` = the first preset of the bank. Forgotten when another bank is loaded.
	pub preset: Option<(u16, u8)>,
	pub reverb: bool,
	pub chorus: bool,
	/// Send levels (CC 91 / CC 93), 0..=127, applied to every channel while the effect is on (F-13).
	pub reverb_level: u8,
	pub chorus_level: u8,
	/// Which processor plays the notes (spec v1.2, F-22).
	pub engine: EngineKind,
	/// The timbre of the built-in synthesizer. A table: it stays the last field, as TOML wants.
	pub builtin: BuiltinConfig,
}

impl Default for SynthConfig {
	fn default() -> Self {
		Self {
			sf2: None,
			recent_banks: Vec::new(),
			preset: None,
			reverb: true,
			chorus: false,
			reverb_level: 40,
			chorus_level: 20,
			engine: EngineKind::default(),
			builtin: BuiltinConfig::default(),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerConfig {
	/// Master volume in percent, 0..=100.
	pub master: u8,
}

impl Default for MixerConfig {
	fn default() -> Self {
		Self { master: 80 }
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardConfig {
	pub base_octave: u8,
	pub velocity: u8,
	/// White keys left to right starting at C; black keys for C#, D#, F#, G#, A#, C#, D#. Keys are
	/// named by the character a US keyboard has in that place (spec v1.2 §4.3).
	pub white: String,
	pub black: String,
	/// Extra chords for octave / velocity, in addition to Ctrl+arrows (e.g. "F11", "Alt+Up").
	pub octave_up: Option<String>,
	pub octave_down: Option<String>,
	pub velocity_up: Option<String>,
	pub velocity_down: Option<String>,
}

impl Default for KeyboardConfig {
	fn default() -> Self {
		Self {
			base_octave: 4,
			velocity: 100,
			white: "zxcvbnm,./".to_owned(),
			black: "sdghjl;".to_owned(),
			octave_up: None,
			octave_down: None,
			velocity_up: None,
			velocity_down: None,
		}
	}
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
	/// "ru" or "en"; `None` follows the system locale (spec F-19).
	pub lang: Option<String>,
	/// Font family of the window; `None` or an unavailable family = the system monospace font (spec v1.2 N-14).
	pub font: Option<String>,
	/// Logical font size of the window in pixels; `None` = the built-in default (spec v1.2 N-14).
	pub font_size: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordFormat {
	Flac,
	OggVorbis,
	/// The events, not the sound: a Standard MIDI File (spec v1.2, F-23).
	Midi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordConfig {
	/// `None` → the OS music/data directory, resolved by the recorder.
	pub dir: Option<PathBuf>,
	pub format: RecordFormat,
	/// Ogg Vorbis quality 0.0..=1.0.
	pub vorbis_quality: f32,
	/// Folders chosen before, most recent first, at most `RECENT_DIRS_LIMIT` entries.
	pub recent_dirs: Vec<PathBuf>,
}

impl Default for RecordConfig {
	fn default() -> Self {
		Self {
			dir: None,
			format: RecordFormat::Flac,
			vorbis_quality: 0.5,
			recent_dirs: Vec::new(),
		}
	}
}

impl Config {
	/// Loads an explicitly named file; the file must exist.
	pub fn load(path: &Path) -> Result<Self, ConfigError> {
		let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
			path: path.to_owned(),
			source,
		})?;
		toml::from_str(&text).map_err(|source| ConfigError::Parse {
			path: path.to_owned(),
			source,
		})
	}

	/// Loads the file if it exists, otherwise returns defaults; a present but broken file is still an error.
	pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
		if path.exists() { Self::load(path) } else { Ok(Self::default()) }
	}

	pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
				path: path.to_owned(),
				source,
			})?;
		}
		let text = toml::to_string_pretty(self)?;
		std::fs::write(path, text).map_err(|source| ConfigError::Write {
			path: path.to_owned(),
			source,
		})
	}

	/// Puts `path` at the front of the recent list, deduplicated and truncated to `RECENT_BANKS_LIMIT`.
	pub fn remember_bank(&mut self, path: &Path) {
		remember(&mut self.synth.recent_banks, path, RECENT_BANKS_LIMIT);
	}

	/// Makes `path` the MIDI file to play and puts it at the front of the recent list.
	pub fn set_midi_file(&mut self, path: &Path) {
		remember(&mut self.source.recent_midi_files, path, RECENT_MIDI_FILES_LIMIT);
		self.source.midi_file = Some(path.to_owned());
	}

	/// Sets where takes go; `None` is the OS music directory. A chosen folder goes to the front of the
	/// recent list, which is the history of the folder dialog (spec v1.2 §4.2).
	pub fn set_record_dir(&mut self, dir: Option<PathBuf>) {
		if let Some(dir) = &dir {
			remember(&mut self.record.recent_dirs, dir, RECENT_DIRS_LIMIT);
		}
		self.record.dir = dir;
	}
}

fn remember(recent: &mut Vec<PathBuf>, path: &Path, limit: usize) {
	recent.retain(|known| known != path);
	recent.insert(0, path.to_owned());
	recent.truncate(limit);
}

/// Standard per-OS directories for the application (`directories` conventions, spec N-06).
pub fn project_dirs() -> Option<ProjectDirs> {
	ProjectDirs::from("", "", APP_NAME)
}

pub fn default_config_path() -> Option<PathBuf> {
	project_dirs().map(|dirs| dirs.config_dir().join(CONFIG_FILE_NAME))
}

/// Start-up settings after merging: CLI flag > config value > built-in default.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
	pub sf2: Option<PathBuf>,
	pub midi_in: Option<DeviceSelector>,
	pub audio_out: Option<DeviceSelector>,
	pub buffer_frames: u32,
	pub sample_rate: Option<u32>,
}

impl Settings {
	pub fn resolve(cli: &Cli, config: &Config) -> Result<Self, ConfigError> {
		let midi_in = match &cli.midi_in {
			Some(selector) => Some(selector.clone()),
			None => parse_selector(config.midi.input.as_deref(), "midi.input")?,
		};
		let audio_out = match &cli.audio_out {
			Some(selector) => Some(selector.clone()),
			None => parse_selector(config.audio.output.as_deref(), "audio.output")?,
		};
		Ok(Self {
			sf2: cli.sf2.clone().or_else(|| config.synth.sf2.clone()),
			midi_in,
			audio_out,
			buffer_frames: cli.buffer.unwrap_or(config.audio.buffer_frames),
			sample_rate: cli.sample_rate.or(config.audio.sample_rate),
		})
	}
}

fn parse_selector(raw: Option<&str>, key: &'static str) -> Result<Option<DeviceSelector>, ConfigError> {
	raw.map(|text| text.parse().map_err(|source| ConfigError::DeviceSelector { key, source }))
		.transpose()
}

#[cfg(test)]
mod tests;
