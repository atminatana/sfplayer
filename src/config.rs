//! TOML configuration (spec N-06) in the OS config directory; section layout for keys: ADR 0007.
//!
//! Every field has a default and unknown keys are ignored, so an old or partial file always loads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, DEFAULT_BUFFER_FRAMES, DeviceSelector, EmptyDeviceSelector};

pub const APP_NAME: &str = "sf2_player";
const CONFIG_FILE_NAME: &str = "config.toml";
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthConfig {
	pub sf2: Option<PathBuf>,
	/// Most recent first, at most `RECENT_BANKS_LIMIT` entries.
	pub recent_banks: Vec<PathBuf>,
	pub reverb: bool,
	pub chorus: bool,
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
	/// Note duration in gate mode, when the terminal cannot report key release (spec F-16).
	pub gate_ms: u32,
	pub base_octave: u8,
	pub velocity: u8,
	/// White keys left to right starting at C; black keys for C#, D#, F#, G#, A#, C#, D#.
	pub white: String,
	pub black: String,
}

impl Default for KeyboardConfig {
	fn default() -> Self {
		Self {
			gate_ms: 300,
			base_octave: 4,
			velocity: 100,
			white: "zxcvbnm,./".to_owned(),
			black: "sdghjl;".to_owned(),
		}
	}
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
	/// "ru" or "en"; `None` follows the system locale (spec F-19).
	pub lang: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordFormat {
	Flac,
	OggVorbis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordConfig {
	/// `None` → the OS music/data directory, resolved by the recorder.
	pub dir: Option<PathBuf>,
	pub format: RecordFormat,
	/// Ogg Vorbis quality 0.0..=1.0.
	pub vorbis_quality: f32,
}

impl Default for RecordConfig {
	fn default() -> Self {
		Self {
			dir: None,
			format: RecordFormat::Flac,
			vorbis_quality: 0.5,
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
		self.synth.recent_banks.retain(|known| known != path);
		self.synth.recent_banks.insert(0, path.to_owned());
		self.synth.recent_banks.truncate(RECENT_BANKS_LIMIT);
	}
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
mod tests {
	use std::sync::atomic::{AtomicU32, Ordering};

	use clap::Parser;

	use super::*;

	static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

	/// Unique path in the OS temp directory; the file does not exist until a test writes it.
	fn temp_path(suffix: &str) -> PathBuf {
		let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
		std::env::temp_dir().join(format!("sf2_player-test-{}-{unique}-{suffix}", std::process::id()))
	}

	#[test]
	fn empty_text_gives_defaults() {
		let config: Config = toml::from_str("").expect("empty TOML parses");
		assert_eq!(config, Config::default());
		assert_eq!(config.audio.buffer_frames, DEFAULT_BUFFER_FRAMES);
		assert_eq!(config.mixer.master, 80);
		assert_eq!(config.keyboard.gate_ms, 300);
		assert_eq!(config.record.format, RecordFormat::Flac);
	}

	#[test]
	fn unknown_keys_and_partial_sections_are_tolerated() {
		let text = "[audio]\nbuffer_frames = 128\nfuture_option = true\n[nonsense]\nx = 1\n";
		let config: Config = toml::from_str(text).expect("parses");
		assert_eq!(config.audio.buffer_frames, 128);
		assert_eq!(config.audio.sample_rate, None);
	}

	#[test]
	fn save_then_load_round_trips() {
		let path = temp_path("roundtrip").join("nested").join("config.toml");
		let mut config = Config::default();
		config.midi.input = Some("KeyStep".into());
		config.audio.sample_rate = Some(44_100);
		config.synth.sf2 = Some(PathBuf::from("bank.sf2"));
		config.keys.insert("panic".into(), "F9".into());
		config.record.format = RecordFormat::OggVorbis;
		config.save(&path).expect("save");
		let loaded = Config::load(&path).expect("load");
		assert_eq!(loaded, config);
		std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
	}

	#[test]
	fn missing_file_is_default_or_error_depending_on_entry_point() {
		let path = temp_path("missing.toml");
		assert_eq!(Config::load_or_default(&path).expect("defaults"), Config::default());
		assert!(matches!(Config::load(&path), Err(ConfigError::Read { .. })));
	}

	#[test]
	fn broken_file_is_parse_error_even_when_optional() {
		let path = temp_path("broken.toml");
		std::fs::write(&path, "[audio\nbuffer_frames = ").expect("write");
		assert!(matches!(Config::load_or_default(&path), Err(ConfigError::Parse { .. })));
		std::fs::remove_file(&path).ok();
	}

	#[test]
	fn remember_bank_moves_to_front_dedupes_and_truncates() {
		let mut config = Config::default();
		for index in 0..RECENT_BANKS_LIMIT + 3 {
			config.remember_bank(Path::new(&format!("bank{index}.sf2")));
		}
		config.remember_bank(Path::new("bank5.sf2"));
		assert_eq!(config.synth.recent_banks.len(), RECENT_BANKS_LIMIT);
		assert_eq!(config.synth.recent_banks[0], PathBuf::from("bank5.sf2"));
		assert_eq!(
			config
				.synth
				.recent_banks
				.iter()
				.filter(|p| p.as_path() == Path::new("bank5.sf2"))
				.count(),
			1
		);
	}

	#[test]
	fn cli_overrides_config_which_overrides_defaults() {
		let mut config = Config::default();
		config.midi.input = Some("2".into());
		config.audio.output = Some("Speakers".into());
		config.audio.buffer_frames = 128;
		config.synth.sf2 = Some(PathBuf::from("from-config.sf2"));

		let cli = Cli::parse_from(["sf2_player", "--midi-in", "KeyStep", "--buffer", "512"]);
		let settings = Settings::resolve(&cli, &config).expect("resolve");
		assert_eq!(settings.midi_in, Some(DeviceSelector::Name("KeyStep".into())));
		assert_eq!(settings.audio_out, Some(DeviceSelector::Name("Speakers".into())));
		assert_eq!(settings.buffer_frames, 512);
		assert_eq!(settings.sf2, Some(PathBuf::from("from-config.sf2")));

		let bare = Cli::parse_from(["sf2_player"]);
		let settings = Settings::resolve(&bare, &config).expect("resolve");
		assert_eq!(settings.midi_in, Some(DeviceSelector::Index(2)));
		assert_eq!(settings.buffer_frames, 128);

		let settings = Settings::resolve(&bare, &Config::default()).expect("resolve");
		assert_eq!(settings.buffer_frames, DEFAULT_BUFFER_FRAMES);
		assert_eq!(settings.midi_in, None);
	}

	#[test]
	fn blank_selector_in_config_is_error() {
		let mut config = Config::default();
		config.audio.output = Some("  ".into());
		let cli = Cli::parse_from(["sf2_player"]);
		assert!(matches!(
			Settings::resolve(&cli, &config),
			Err(ConfigError::DeviceSelector { key: "audio.output", .. })
		));
	}
}
