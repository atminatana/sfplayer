//! Tests of the config: defaults, reading and writing, precedence of the command line, the timbre.

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
	assert_eq!(config.keyboard.base_octave, 4);
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
	config.synth.preset = Some((128, 5));
	config.keys.insert("panic".into(), "F9".into());
	config.record.format = RecordFormat::OggVorbis;
	config.save(&path).expect("save");
	let loaded = Config::load(&path).expect("load");
	assert_eq!(loaded, config);
	// `to_string_pretty` may break the array over several lines; the shape `preset = [128, 5]` is what matters.
	let compact: String = std::fs::read_to_string(&path).expect("read back").split_whitespace().collect();
	assert!(
		compact.contains("preset=[128,5,]") || compact.contains("preset=[128,5]"),
		"{compact}"
	);
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
fn folders_for_takes_are_remembered_like_banks_and_survive_a_save() {
	let mut config = Config::default();
	for index in 0..RECENT_DIRS_LIMIT + 2 {
		config.set_record_dir(Some(PathBuf::from(format!("takes{index}"))));
	}
	config.set_record_dir(Some(PathBuf::from("takes4")));
	assert_eq!(config.record.dir, Some(PathBuf::from("takes4")));
	// Back to the default folder: the choice is dropped, the history stays.
	config.set_record_dir(None);
	assert_eq!(config.record.dir, None);
	assert_eq!(config.record.recent_dirs.len(), RECENT_DIRS_LIMIT);
	assert_eq!(config.record.recent_dirs[0], PathBuf::from("takes4"));
	assert_eq!(
		config
			.record
			.recent_dirs
			.iter()
			.filter(|dir| dir.as_path() == Path::new("takes4"))
			.count(),
		1
	);
	let path = temp_path("recent_dirs").join("config.toml");
	config.save(&path).expect("save");
	assert_eq!(Config::load(&path).expect("load").record.recent_dirs, config.record.recent_dirs);
	std::fs::remove_dir_all(path.parent().unwrap()).ok();
	// A config written before the list existed opens with an empty one.
	let old: Config = toml::from_str("[record]\nformat = \"flac\"\n").expect("parse");
	assert!(old.record.recent_dirs.is_empty());
}

#[test]
fn the_processor_and_the_timbre_default_to_a_sound_font_and_a_piano_sine() {
	let config = Config::default();
	assert_eq!(config.synth.engine, EngineKind::Sf2);
	let builtin = &config.synth.builtin;
	assert_eq!((builtin.wave, builtin.adsr_preset), (Wave::Sine, AdsrPreset::Piano));
	assert_eq!(
		(builtin.attack, builtin.decay, builtin.sustain, builtin.release),
		(0.003, 0.5, 0.35, 0.35)
	);
	assert_eq!(EngineKind::Sf2.other(), EngineKind::Builtin);
	assert_eq!(EngineKind::Builtin.other(), EngineKind::Sf2);
}

#[test]
fn the_processor_and_the_timbre_are_written_as_the_spec_names_them_and_read_back() {
	let mut config = Config::default();
	config.synth.engine = EngineKind::Builtin;
	config.synth.builtin.wave = Wave::Triangle;
	config.synth.builtin.apply_preset(AdsrPreset::Flute);
	let path = temp_path("builtin").join("config.toml");
	config.save(&path).expect("save");
	let written = std::fs::read_to_string(&path).expect("read back");
	for line in [
		"engine = \"builtin\"",
		"[synth.builtin]",
		"wave = \"triangle\"",
		"adsr_preset = \"flute\"",
		"sustain = 0.85",
	] {
		assert!(written.lines().any(|written| written == line), "{line:?} in:{written}");
	}
	assert_eq!(Config::load(&path).expect("load"), config);
	std::fs::remove_dir_all(path.parent().unwrap()).ok();

	// A config from before the built-in synthesizer opens on a Sound Font with the default timbre,
	// and a partly written section keeps the defaults for the rest.
	let old: Config = toml::from_str("[synth]\nreverb = false\n").expect("parse");
	assert_eq!((old.synth.engine, &old.synth.builtin), (EngineKind::Sf2, &BuiltinConfig::default()));
	let partial: Config = toml::from_str("[synth]\nengine = \"builtin\"\n[synth.builtin]\nwave = \"saw\"\n").expect("parse");
	assert_eq!((partial.synth.engine, partial.synth.builtin.wave), (EngineKind::Builtin, Wave::Saw));
	assert_eq!(partial.synth.builtin.adsr(), BuiltinConfig::default().adsr());
}

#[test]
fn a_ready_made_envelope_brings_its_values_and_a_hand_made_one_keeps_them() {
	let mut timbre = BuiltinConfig::default();
	for (preset, values) in [
		(AdsrPreset::Piano, (0.003, 0.5, 0.35, 0.35)),
		(AdsrPreset::Harpsichord, (0.002, 0.28, 0.2, 0.18)),
		(AdsrPreset::Organ, (0.03, 0.06, 1.0, 0.09)),
		(AdsrPreset::Flute, (0.12, 0.09, 0.85, 0.22)),
	] {
		timbre.apply_preset(preset);
		assert_eq!(timbre.adsr_preset, preset);
		assert_eq!((timbre.attack, timbre.decay, timbre.sustain, timbre.release), values, "{preset:?}");
	}
	assert_eq!(AdsrPreset::READY_MADE.len(), 4);
	timbre.apply_preset(AdsrPreset::Custom);
	assert_eq!(timbre.adsr_preset, AdsrPreset::Custom);
	assert_eq!(timbre.sustain, 0.85, "the values stay as they were");
}

#[test]
fn a_step_moves_one_value_inside_its_limits_and_makes_the_envelope_the_users_own() {
	let mut timbre = BuiltinConfig::default();
	timbre.adjust(AdsrField::Attack, 1);
	assert_eq!(timbre.adsr_preset, AdsrPreset::Custom);
	assert_eq!(
		(timbre.attack, timbre.decay, timbre.sustain, timbre.release),
		(0.008, 0.5, 0.35, 0.35)
	);
	timbre.adjust(AdsrField::Decay, -1);
	timbre.adjust(AdsrField::Sustain, 1);
	timbre.adjust(AdsrField::Release, 2);
	assert_eq!(
		(timbre.attack, timbre.decay, timbre.sustain, timbre.release),
		(0.008, 0.495, 0.4, 0.36)
	);

	// Many steps do not drift: twenty up and twenty down come back to the same value.
	for _ in 0..20 {
		timbre.adjust(AdsrField::Release, 1);
	}
	for _ in 0..20 {
		timbre.adjust(AdsrField::Release, -1);
	}
	assert_eq!(timbre.release, 0.36);

	// The limits: a millisecond to two seconds, nothing to all.
	timbre.adjust(AdsrField::Attack, -100);
	assert_eq!(timbre.attack, 0.001);
	timbre.adjust(AdsrField::Attack, 1);
	assert_eq!(timbre.attack, 0.006, "a step up from the lowest time");
	for _ in 0..500 {
		timbre.adjust(AdsrField::Decay, 1);
	}
	assert_eq!(timbre.decay, 2.0);
	timbre.adjust(AdsrField::Sustain, 100);
	assert_eq!(timbre.sustain, 1.0);
	timbre.adjust(AdsrField::Sustain, -100);
	assert_eq!(timbre.sustain, 0.0);

	// Values written by hand into the file are brought inside the limits when the engine takes them.
	let wild = BuiltinConfig {
		attack: 0.0,
		decay: 9.0,
		sustain: 3.0,
		release: -1.0,
		..BuiltinConfig::default()
	};
	let adsr = wild.adsr();
	assert_eq!((adsr.attack, adsr.decay, adsr.sustain, adsr.release), (0.001, 2.0, 1.0, 0.001));
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

#[test]
fn the_midi_file_and_the_recent_ones_are_kept_in_the_source_section() {
	let mut config = Config::default();
	assert_eq!(config.source, SourceConfig::default());
	assert_eq!((config.source.midi_file.clone(), config.source.recent_midi_files.len()), (None, 0));
	for index in 0..RECENT_MIDI_FILES_LIMIT + 2 {
		config.set_midi_file(Path::new(&format!("song{index}.mid")));
	}
	config.set_midi_file(Path::new("song3.mid"));
	assert_eq!(config.source.midi_file, Some(PathBuf::from("song3.mid")));
	assert_eq!(config.source.recent_midi_files.len(), RECENT_MIDI_FILES_LIMIT);
	assert_eq!(RECENT_MIDI_FILES_LIMIT, 10, "the spec keeps ten");
	assert_eq!(config.source.recent_midi_files[0], PathBuf::from("song3.mid"));
	assert_eq!(
		config
			.source
			.recent_midi_files
			.iter()
			.filter(|path| path.as_path() == Path::new("song3.mid"))
			.count(),
		1
	);

	let path = temp_path("source").join("config.toml");
	config.save(&path).expect("save");
	let written = std::fs::read_to_string(&path).expect("read back");
	assert!(written.lines().any(|line| line == "[source]"), "{written}");
	assert!(written.lines().any(|line| line == "midi_file = \"song3.mid\""), "{written}");
	assert_eq!(Config::load(&path).expect("load"), config);
	std::fs::remove_dir_all(path.parent().unwrap()).ok();
	// A config from before the section existed opens without a file.
	let old: Config = toml::from_str("[midi]\ninput = \"KeyStep\"\n").expect("parse");
	assert_eq!(old.source, SourceConfig::default());
}

#[test]
fn the_midi_format_is_spelled_midi_in_the_file() {
	let config: Config = toml::from_str("[record]\nformat = \"midi\"\n").expect("parses");
	assert_eq!(config.record.format, RecordFormat::Midi);
	let written = toml::to_string_pretty(&config).expect("serialises");
	assert!(written.lines().any(|line| line == "format = \"midi\""), "{written}");
	// The names of v1.0 are kept.
	for (name, format) in [("flac", RecordFormat::Flac), ("ogg_vorbis", RecordFormat::OggVorbis)] {
		let config: Config = toml::from_str(&format!("[record]\nformat = \"{name}\"\n")).expect("parses");
		assert_eq!(config.record.format, format);
	}
}
