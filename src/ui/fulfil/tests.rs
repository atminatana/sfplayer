//! What `fulfil` does with each request, on a session that has opened no device. The requests that
//! enumerate or open real devices (a rescan, a switch of the audio output) cannot be reached from a
//! test.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::config::{AdsrField, Config, EngineKind, RecordFormat};
use crate::midi::MidiEvent;
use crate::session::{EffectsSettings, MixerAction};
use crate::synth::{AdsrPreset, Wave};
use crate::ui::app::KEYBOARD_CHANNEL;
use crate::ui::dialog::DialogKind;

static CONFIG_COUNTER: AtomicU32 = AtomicU32::new(0);

/// The config file of one test, in the OS temp directory; removed when the test ends.
struct ConfigFile(PathBuf);

impl ConfigFile {
	fn new() -> Self {
		let unique = CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed);
		Self(std::env::temp_dir().join(format!("sf2_player-fulfil-{}-{unique}.toml", std::process::id())))
	}

	/// The config as it was last written; `None` while nothing has saved it.
	fn saved(&self) -> Option<Config> {
		let text = std::fs::read_to_string(&self.0).ok()?;
		Some(toml::from_str(&text).expect("the saved config parses"))
	}
}

impl Drop for ConfigFile {
	fn drop(&mut self) {
		std::fs::remove_file(&self.0).ok();
	}
}

fn session_with(config: Config, file: &ConfigFile) -> Session {
	Session::detached(config, file.0.clone())
}

fn sine_bank() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("sin_wave.sf2")
}

#[test]
fn a_master_key_changes_the_volume_in_the_session_and_in_the_config() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	let before = session.mixer.master;
	fulfil(Request::Mixer(MixerAction::MasterDownFast), &mut app, &mut session);
	assert!(session.mixer.master < before);
	assert_eq!(session.config.mixer.master, session.mixer.master);
	assert_eq!(file.saved(), None, "the volume is written when the program closes");
}

#[test]
fn the_preset_dialog_opens_only_when_a_bank_is_loaded() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::OpenPresetDialog, &mut app, &mut session);
	assert_eq!(app.modal, None);

	session.load_bank(&sine_bank()).expect("the test bank loads");
	fulfil(Request::OpenPresetDialog, &mut app, &mut session);
	let Some(Modal::Select(dialog)) = &app.modal else {
		panic!("the preset dialog is open");
	};
	assert_eq!(dialog.kind, DialogKind::Preset);
	assert!(!dialog.selector.visible().is_empty());
}

#[test]
fn the_folder_dialog_is_asked_with_the_folder_and_its_history() {
	let file = ConfigFile::new();
	let mut config = Config::default();
	config.record.dir = Some(PathBuf::from("takes"));
	config.record.recent_dirs = vec![PathBuf::from("takes"), PathBuf::from("older")];
	let mut session = session_with(config, &file);
	let mut app = App::new("en");
	fulfil(Request::OpenRecordDialog, &mut app, &mut session);
	assert_eq!(
		app.path_request,
		Some(PathRequest {
			kind: InputKind::RecordDir,
			initial: "takes".to_owned(),
			history: vec![PathBuf::from("takes"), PathBuf::from("older")],
		})
	);
}

#[test]
fn the_bank_dialog_starts_from_the_loaded_bank_and_the_recent_ones() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::OpenSf2Dialog, &mut app, &mut session);
	assert_eq!(
		app.path_request,
		Some(PathRequest {
			kind: InputKind::Sf2File,
			initial: String::new(),
			history: Vec::new(),
		})
	);

	session.load_bank(&sine_bank()).expect("the test bank loads");
	fulfil(Request::OpenSf2Dialog, &mut app, &mut session);
	let request = app.path_request.take().expect("the bank dialog is asked for");
	assert_eq!(request.initial, sine_bank().to_string_lossy());
	assert_eq!(request.history, vec![sine_bank()]);
}

#[test]
fn the_midi_file_dialog_starts_from_the_remembered_file() {
	let file = ConfigFile::new();
	let mut config = Config::default();
	config.source.midi_file = Some(PathBuf::from("song.mid"));
	config.source.recent_midi_files = vec![PathBuf::from("song.mid")];
	let mut session = session_with(config, &file);
	let mut app = App::new("en");
	fulfil(Request::OpenMidiFileDialog, &mut app, &mut session);
	assert_eq!(
		app.path_request,
		Some(PathRequest {
			kind: InputKind::MidiFile,
			initial: "song.mid".to_owned(),
			history: vec![PathBuf::from("song.mid")],
		})
	);
}

#[test]
fn the_effects_dialog_opens_with_the_current_settings() {
	let file = ConfigFile::new();
	let mut config = Config::default();
	config.synth.reverb = true;
	config.synth.reverb_level = 55;
	let mut session = session_with(config, &file);
	let mut app = App::new("en");
	fulfil(Request::OpenEffectsDialog, &mut app, &mut session);
	assert_eq!(app.modal, Some(Modal::Effects(EffectsDialog::new(session.effects()))));
	assert_eq!(session.effects().reverb_level, 55);
}

#[test]
fn a_chosen_preset_is_applied_and_saved() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::Apply(Choice::SelectPreset { bank: 0, program: 7 }), &mut app, &mut session);
	assert_eq!(session.preset_for(), Some((0, 7)));
	assert_eq!(file.saved().expect("saved").synth.preset, Some((0, 7)));
}

/// Without a ring to the audio thread the port is not connected, and the config is saved all the same.
#[test]
fn a_chosen_midi_port_saves_the_config() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::Apply(Choice::ConnectMidi(0)), &mut app, &mut session);
	assert_eq!(session.connected_midi_port(), None);
	assert!(file.saved().is_some());
}

#[test]
fn the_record_format_is_set_and_saved() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::SetRecordFormat(RecordFormat::Midi), &mut app, &mut session);
	assert_eq!(session.config.record.format, RecordFormat::Midi);
	assert_eq!(file.saved().expect("saved").record.format, RecordFormat::Midi);
}

#[test]
fn a_change_of_the_processor_drops_the_held_notes_and_is_saved() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	assert!(!app.keyboard.press('z', KEYBOARD_CHANNEL).is_empty(), "a note is held");
	fulfil(Request::SetEngine(EngineKind::Builtin), &mut app, &mut session);
	assert!(app.keyboard.all_notes_off().is_empty(), "the held note went with the old engine");
	assert_eq!(session.config.synth.engine, EngineKind::Builtin);
	assert_eq!(file.saved().expect("saved").synth.engine, EngineKind::Builtin);

	assert!(!app.keyboard.press('z', KEYBOARD_CHANNEL).is_empty());
	fulfil(Request::ToggleEngine, &mut app, &mut session);
	assert!(app.keyboard.all_notes_off().is_empty());
	assert_eq!(session.config.synth.engine, EngineKind::Sf2);
	assert_eq!(file.saved().expect("saved").synth.engine, EngineKind::Sf2);
}

#[test]
fn timbre_requests_change_the_config_and_wait_for_the_exit_to_be_saved() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::SetWave(Wave::Saw), &mut app, &mut session);
	assert_eq!(session.config.synth.builtin.wave, Wave::Saw);

	let piano = session.config.synth.builtin.adsr();
	fulfil(Request::SetAdsrPreset(AdsrPreset::Organ), &mut app, &mut session);
	let organ = session.config.synth.builtin.adsr();
	assert_ne!(organ, piano);

	fulfil(Request::AdjustAdsr(AdsrField::Attack, 1), &mut app, &mut session);
	assert_ne!(session.config.synth.builtin.adsr(), organ);
	assert_eq!(file.saved(), None);
}

#[test]
fn transposition_and_effect_switches_reach_the_session() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	fulfil(Request::Transpose(1), &mut app, &mut session);
	fulfil(Request::Transpose(1), &mut app, &mut session);
	assert_eq!(session.transpose, 2);

	let was_on = session.effects().reverb;
	fulfil(Request::Effects(EffectsAction::ToggleReverb), &mut app, &mut session);
	assert_eq!(session.effects().reverb, !was_on);
	assert_eq!(file.saved(), None, "a switch alone does not write the config");
}

#[test]
fn saved_effects_are_applied_and_written() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	let settings = EffectsSettings {
		reverb: true,
		chorus: true,
		reverb_level: 30,
		chorus_level: 90,
	};
	fulfil(Request::SaveEffects(settings), &mut app, &mut session);
	assert_eq!(session.effects(), settings);
	let saved = file.saved().expect("saved");
	assert_eq!((saved.synth.reverb_level, saved.synth.chorus_level), (30, 90));
}

/// Recording, the file transport, notes and panic all need the audio stream; without one they are
/// asked for and nothing happens — no take, no error.
#[test]
fn requests_that_need_the_audio_stream_do_nothing_without_it() {
	let file = ConfigFile::new();
	let mut session = session_with(Config::default(), &file);
	let mut app = App::new("en");
	for request in [
		Request::StartRecord,
		Request::StopRecord,
		Request::CancelRecord,
		Request::StopRecordingAndQuit,
		Request::FilePlay,
		Request::FilePause,
		Request::FilePlayPause,
		Request::FileStop,
		Request::Panic,
		Request::Notes(vec![MidiEvent::NoteOn {
			channel: 0,
			key: 60,
			velocity: 100,
		}]),
	] {
		fulfil(request, &mut app, &mut session);
	}
	assert_eq!(session.recording_elapsed(), None);
	assert_eq!(session.notice, None);
	assert_eq!(app.modal, None);
	assert_eq!(file.saved(), None);
}
