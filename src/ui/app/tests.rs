//! Key handling and request tests for `App` (no devices involved).

use super::*;
use crate::midi::MidiEvent;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
	press_kind(code, modifiers, KeyEventKind::Press)
}

fn press_kind(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
	let mut key = KeyEvent::new(code, modifiers);
	key.kind = kind;
	key
}

fn plain(code: KeyCode) -> KeyEvent {
	press(code, KeyModifiers::NONE)
}

#[test]
fn the_quit_key_sets_the_flag() {
	let mut app = App::new("en");
	app.handle_key(plain(KeyCode::F(10)));
	assert!(app.should_quit);
}

#[test]
fn unbound_keys_do_nothing_and_need_no_redraw() {
	let mut app = App::new("en");
	app.take_dirty();
	assert_eq!(app.handle_key(plain(KeyCode::Char('q'))), None, "q is neither a command nor a note");
	assert_eq!(app.handle_key(plain(KeyCode::Tab)), None, "Tab is no longer a command (ADR 0010)");
	assert_eq!(app.handle_key(plain(KeyCode::Up)), None, "arrows mean nothing on the main screen");
	assert_eq!(app.handle_key(plain(KeyCode::Char('3'))), None, "digits no longer pick a channel");
	assert!(!app.take_dirty());
	assert!(!app.should_quit);
}

#[test]
fn function_keys_request_dialogs_and_rescan() {
	let mut app = App::new("en");
	// The devices are chosen from dropdowns of the screen: their keys ask the window to open one.
	assert_eq!(app.handle_key(plain(KeyCode::F(2))), None);
	assert_eq!(app.dropdown_request.take(), Some(Dropdown::MidiIn));
	assert_eq!(app.handle_key(plain(KeyCode::F(3))), None);
	assert_eq!(app.dropdown_request.take(), Some(Dropdown::AudioOut));
	assert!(app.take_dirty());
	assert_eq!(app.handle_key(plain(KeyCode::F(4))), Some(Request::OpenSf2Dialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(5))), Some(Request::OpenPresetDialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(6))), Some(Request::OpenEffectsDialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(7))), Some(Request::OpenRecordDialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(8))), Some(Request::Rescan));
}

#[test]
fn master_keys_work_everywhere() {
	let mut app = App::new("en");
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('+'))),
		Some(Request::Mixer(MixerAction::MasterUp))
	);
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('-'))),
		Some(Request::Mixer(MixerAction::MasterDown))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('+'), KeyModifiers::SHIFT)),
		Some(Request::Mixer(MixerAction::MasterUpFast))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('u'), KeyModifiers::CONTROL)),
		Some(Request::Mixer(MixerAction::MasterMute))
	);
	assert_eq!(app.handle_key(plain(KeyCode::Right)), None, "channel keys are gone with the strips");
	assert_eq!(app.handle_key(press(KeyCode::Char('x'), KeyModifiers::CONTROL)), None);
}

#[test]
fn f9_requests_panic_and_the_midi_file_has_no_keys_of_its_own() {
	let mut app = App::new("en");
	assert_eq!(app.handle_key(plain(KeyCode::F(9))), Some(Request::Panic));
	// The file is driven by its buttons: the keys it had in v1.2 are free (spec v1.3, F-28).
	assert_eq!(app.handle_key(plain(KeyCode::Char(' '))), None);
	assert_eq!(app.handle_key(plain(KeyCode::Backspace)), None);
	assert_eq!(app.handle_key(plain(KeyCode::F(11))), None);
}

#[test]
fn f1_asks_for_the_help() {
	let mut app = App::new("en");
	app.take_dirty();
	assert_eq!(app.handle_key(plain(KeyCode::F(1))), None);
	assert!(app.is_help_requested && app.take_dirty());
}

#[test]
fn text_keyboard_plays_notes_on_channel_one_and_stops_on_release_or_esc() {
	let mut app = App::new("en");
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('z'))),
		Some(Request::Notes(vec![MidiEvent::NoteOn {
			channel: 0,
			key: 60,
			velocity: 100
		}]))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('S'), KeyModifiers::SHIFT)),
		Some(Request::Notes(vec![MidiEvent::NoteOn {
			channel: 0,
			key: 61,
			velocity: 100
		}])),
		"shifted letters still play"
	);
	assert_eq!(
		app.handle_key(press_kind(KeyCode::Char('z'), KeyModifiers::NONE, KeyEventKind::Repeat)),
		None,
		"auto-repeat is ignored"
	);
	assert_eq!(
		app.handle_release(press_kind(KeyCode::Char('z'), KeyModifiers::NONE, KeyEventKind::Release)),
		Some(Request::Notes(vec![MidiEvent::NoteOff { channel: 0, key: 60 }])),
		"the note stops when the key goes up"
	);
	assert_eq!(
		app.handle_release(press_kind(KeyCode::Char('z'), KeyModifiers::NONE, KeyEventKind::Release)),
		None,
		"a second release has nothing to stop"
	);
	assert_eq!(
		app.handle_key(plain(KeyCode::Esc)),
		Some(Request::Notes(vec![MidiEvent::NoteOff { channel: 0, key: 61 }]))
	);
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None, "nothing left to release");
}

#[test]
fn ctrl_arrows_change_octave_and_velocity_and_panic_forgets_held_notes() {
	let mut app = App::new("en");
	app.handle_key(press(KeyCode::Up, KeyModifiers::CONTROL));
	app.handle_key(press(KeyCode::Right, KeyModifiers::CONTROL));
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('z'))),
		Some(Request::Notes(vec![MidiEvent::NoteOn {
			channel: 0,
			key: 72,
			velocity: 110
		}]))
	);
	assert_eq!(app.handle_key(plain(KeyCode::F(9))), Some(Request::Panic));
	assert_eq!(app.keyboard.held_count(), 0);
}

#[test]
fn brackets_request_transposition() {
	let mut app = App::new("en");
	assert_eq!(app.handle_key(plain(KeyCode::Char(']'))), Some(Request::Transpose(1)));
	assert_eq!(app.handle_key(plain(KeyCode::Char('['))), Some(Request::Transpose(-1)));
}

#[test]
fn alt_r_and_alt_c_toggle_effects() {
	use crate::session::EffectsAction;
	let mut app = App::new("en");
	assert_eq!(
		app.handle_key(press(KeyCode::Char('r'), KeyModifiers::ALT)),
		Some(Request::Effects(EffectsAction::ToggleReverb))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('c'), KeyModifiers::ALT)),
		Some(Request::Effects(EffectsAction::ToggleChorus))
	);
}

#[test]
fn quit_asks_for_confirmation_while_recording() {
	let mut app = App::new("en");
	// A take is started and stopped by its buttons; Home and End are free (spec v1.3, F-28).
	assert_eq!(app.handle_key(plain(KeyCode::Home)), None);
	assert_eq!(app.handle_key(plain(KeyCode::End)), None);
	app.recording = Some(Duration::from_secs(12));
	assert_eq!(app.handle_key(plain(KeyCode::F(10))), None);
	assert_eq!(app.confirm, Some(Question::Quit));
	assert!(!app.should_quit);
	// The keys of the question belong to its window: nothing is played and no command runs.
	assert_eq!(app.handle_key(plain(KeyCode::Char('z'))), None);
	assert_eq!(app.handle_key(plain(KeyCode::F(9))), None);
	assert_eq!(app.handle_key(plain(KeyCode::Enter)), None);
	assert_eq!(app.confirm, Some(Question::Quit));

	assert_eq!(app.answer(false), None, "a refusal keeps recording");
	assert!(app.confirm.is_none() && !app.should_quit);

	app.handle_key(plain(KeyCode::F(10)));
	assert_eq!(app.answer(true), Some(Request::StopRecordingAndQuit));
	assert!(app.should_quit && app.confirm.is_none());

	let mut twice = App::new("en");
	twice.recording = Some(Duration::from_secs(1));
	twice.handle_key(plain(KeyCode::F(10)));
	twice.handle_key(plain(KeyCode::F(10)));
	assert!(twice.should_quit, "asked again, it quits");

	let mut idle = App::new("en");
	idle.handle_key(plain(KeyCode::F(10)));
	assert!(idle.should_quit, "no confirmation when not recording");
	assert_eq!(idle.answer(true), None, "no question, no answer");
}

/// Spec v1.3, F-27: the Cancel button throws nothing away until the question is answered.
#[test]
fn the_cancel_button_asks_before_a_take_is_thrown_away() {
	let mut app = App::new("en");
	assert_eq!(app.press_button(ButtonId::RecordCancel), None);
	assert_eq!(app.confirm, None, "no take, nothing to ask about");

	app.recording = Some(Duration::from_secs(42));
	assert_eq!(app.press_button(ButtonId::RecordCancel), None, "the take is not touched yet");
	assert_eq!(app.confirm, Some(Question::DeleteTake));
	assert_eq!(app.answer(false), None, "a refusal leaves the take running");
	assert_eq!(app.confirm, None);

	app.press_button(ButtonId::RecordCancel);
	assert_eq!(app.answer(true), Some(Request::CancelRecord));
	assert!(app.confirm.is_none() && !app.should_quit);
}

#[test]
fn a_take_that_ends_by_itself_closes_the_question_about_deleting_it() {
	let mut app = App::new("en");
	app.set_recording(Some(Duration::from_secs(3)));
	app.press_button(ButtonId::RecordCancel);
	app.take_dirty();
	app.set_recording(Some(Duration::from_secs(4)));
	assert_eq!(app.confirm, Some(Question::DeleteTake), "the take runs on: the question stays");
	app.set_recording(None);
	assert_eq!(app.confirm, None);
	assert!(app.take_dirty(), "the window is to be redrawn without the question");

	// The question about quitting is not about the take: it stays.
	app.set_recording(Some(Duration::from_secs(1)));
	app.handle_key(plain(KeyCode::F(10)));
	app.set_recording(None);
	assert_eq!(app.confirm, Some(Question::Quit));
}

#[test]
fn the_quit_command_replaces_the_question_about_a_take() {
	let mut app = App::new("en");
	app.recording = Some(Duration::from_secs(5));
	app.press_button(ButtonId::RecordCancel);
	app.handle_key(plain(KeyCode::F(10)));
	assert_eq!(app.confirm, Some(Question::Quit));
	assert!(!app.should_quit);
}

/// Spec v1.3, F-28: a command that lost its default key runs from the key the config gives it.
#[test]
fn commands_without_a_default_key_run_from_a_configured_one() {
	let keys = [
		("midi_file_dialog", "F11"),
		("engine_toggle", "F12"),
		("file_play_pause", "Space"),
		("file_stop", "Backspace"),
		("record_start", "Home"),
		("record_stop", "End"),
	]
	.into_iter()
	.map(|(name, chord)| (name.to_owned(), chord.to_owned()))
	.collect();
	let mut app = App::new("en");
	app.bindings = Bindings::from_config(&keys, &KeyboardConfig::default()).expect("valid");
	assert_eq!(app.handle_key(plain(KeyCode::F(11))), Some(Request::OpenMidiFileDialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(12))), Some(Request::ToggleEngine));
	assert_eq!(app.handle_key(plain(KeyCode::Char(' '))), Some(Request::FilePlayPause));
	assert_eq!(app.handle_key(plain(KeyCode::Backspace)), Some(Request::FileStop));
	assert_eq!(app.handle_key(plain(KeyCode::Home)), Some(Request::StartRecord));
	assert_eq!(app.handle_key(plain(KeyCode::End)), Some(Request::StopRecord));
}

#[test]
fn every_available_button_gives_the_command_of_its_hotkey() {
	let cases = [
		(ButtonId::Sf2File, Request::OpenSf2Dialog),
		(ButtonId::Preset, Request::OpenPresetDialog),
		(ButtonId::Effects, Request::OpenEffectsDialog),
		(ButtonId::MasterDown, Request::Mixer(MixerAction::MasterDownFast)),
		(ButtonId::MasterUp, Request::Mixer(MixerAction::MasterUpFast)),
		(ButtonId::Folder, Request::OpenRecordDialog),
		(ButtonId::RecordStart, Request::StartRecord),
		(ButtonId::RecordStop, Request::StopRecord),
		(ButtonId::MidiFile, Request::OpenMidiFileDialog),
		(ButtonId::FileStart, Request::FilePlay),
		(ButtonId::FilePause, Request::FilePause),
		(ButtonId::FileStop, Request::FileStop),
		(ButtonId::EngineSynth, Request::SetEngine(EngineKind::Builtin)),
		(ButtonId::EngineSf2, Request::SetEngine(EngineKind::Sf2)),
		(ButtonId::WaveSine, Request::SetWave(Wave::Sine)),
		(ButtonId::WaveSaw, Request::SetWave(Wave::Saw)),
		(ButtonId::WaveTriangle, Request::SetWave(Wave::Triangle)),
		(ButtonId::AdsrPiano, Request::SetAdsrPreset(AdsrPreset::Piano)),
		(ButtonId::AdsrHarpsichord, Request::SetAdsrPreset(AdsrPreset::Harpsichord)),
		(ButtonId::AdsrOrgan, Request::SetAdsrPreset(AdsrPreset::Organ)),
		(ButtonId::AdsrFlute, Request::SetAdsrPreset(AdsrPreset::Flute)),
		(ButtonId::AttackDown, Request::AdjustAdsr(AdsrField::Attack, -1)),
		(ButtonId::AttackUp, Request::AdjustAdsr(AdsrField::Attack, 1)),
		(ButtonId::DecayDown, Request::AdjustAdsr(AdsrField::Decay, -1)),
		(ButtonId::DecayUp, Request::AdjustAdsr(AdsrField::Decay, 1)),
		(ButtonId::SustainDown, Request::AdjustAdsr(AdsrField::Sustain, -1)),
		(ButtonId::SustainUp, Request::AdjustAdsr(AdsrField::Sustain, 1)),
		(ButtonId::ReleaseDown, Request::AdjustAdsr(AdsrField::Release, -1)),
		(ButtonId::ReleaseUp, Request::AdjustAdsr(AdsrField::Release, 1)),
	];
	for (button, request) in &cases {
		let mut app = App::new("en");
		app.take_dirty();
		assert_eq!(app.press_button(*button), Some(request.clone()), "{button:?}");
		assert!(app.take_dirty(), "{button:?} asks for a redraw");
	}
	// The rest wait for their plan stages: pressing them does nothing.
	let waiting: Vec<ButtonId> = ButtonId::ALL
		.into_iter()
		.filter(|button| cases.iter().all(|(known, _)| known != button))
		.collect();
	assert_eq!(waiting.len(), ButtonId::ALL.len() - cases.len());
	for button in waiting {
		assert_eq!(App::new("en").press_button(button), None, "{button:?}");
	}
}

#[test]
fn a_row_of_the_format_dropdown_sets_its_format() {
	let rows = [
		(0, "FLAC", RecordFormat::Flac),
		(1, "OGG", RecordFormat::OggVorbis),
		(2, "MIDI", RecordFormat::Midi),
	];
	for (row, name, format) in rows {
		assert_eq!(RECORD_FORMATS[row], (format, name), "row {row} of the list");
		let mut app = App::new("en");
		app.take_dirty();
		assert_eq!(app.choose_format(row), Some(Request::SetRecordFormat(format)), "{name}");
		assert!(app.take_dirty(), "{name} asks for a redraw");
	}
	let mut app = App::new("en");
	app.take_dirty();
	assert_eq!(app.choose_format(RECORD_FORMATS.len()), None, "a row the list does not have");
	assert!(!app.take_dirty());
}

#[test]
fn the_format_cannot_be_changed_under_a_running_take() {
	let mut app = App::new("en");
	app.recording = Some(Duration::from_secs(1));
	for row in 0..RECORD_FORMATS.len() {
		assert_eq!(app.choose_format(row), None, "row {row}");
	}
	app.recording = None;
	assert_eq!(app.choose_format(1), Some(Request::SetRecordFormat(RecordFormat::OggVorbis)));
}
