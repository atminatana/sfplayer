//! Key handling and request tests for `App` (no devices involved).

use super::*;
use crate::midi::MidiEvent;
use crate::midi::MidiPortInfo;

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

fn midi_dialog() -> Dialog {
	Dialog::midi(
		&[
			MidiPortInfo {
				index: 0,
				name: "KeyStep".into(),
			},
			MidiPortInfo {
				index: 1,
				name: "loopMIDI".into(),
			},
		],
		None,
	)
}

#[test]
fn quit_keys_set_the_flag_even_inside_a_dialog() {
	let mut app = App::new("en");
	app.handle_key(plain(KeyCode::F(10)));
	assert!(app.should_quit);
	let mut app = App::new("en");
	app.open_dialog(midi_dialog());
	app.handle_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL));
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
	assert_eq!(
		app.handle_key(plain(KeyCode::F(2))),
		Some(Request::OpenDialog(DialogKind::MidiInput))
	);
	assert_eq!(
		app.handle_key(plain(KeyCode::F(3))),
		Some(Request::OpenDialog(DialogKind::AudioOutput))
	);
	assert_eq!(app.handle_key(plain(KeyCode::F(4))), Some(Request::OpenSf2Dialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(5))), Some(Request::OpenDialog(DialogKind::Preset)));
	assert_eq!(app.handle_key(plain(KeyCode::F(6))), Some(Request::OpenEffectsDialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(7))), Some(Request::OpenRecordDialog));
	assert_eq!(app.handle_key(plain(KeyCode::F(8))), Some(Request::Rescan));
	assert_eq!(app.handle_key(plain(KeyCode::F(12))), Some(Request::ToggleEngine));
}

#[test]
fn dialog_captures_keys_until_enter_or_esc() {
	let mut app = App::new("en");
	app.open_dialog(midi_dialog());
	assert_eq!(
		app.handle_key(plain(KeyCode::F(3))),
		None,
		"other hotkeys are swallowed while a dialog is open"
	);
	assert!(app.modal.is_some());
	for c in "loop".chars() {
		app.handle_key(plain(KeyCode::Char(c)));
	}
	match &app.modal {
		Some(Modal::Select(dialog)) => assert_eq!(dialog.selector.filter(), "loop"),
		other => panic!("unexpected modal {other:?}"),
	}
	assert_eq!(app.handle_key(plain(KeyCode::Enter)), Some(Request::Apply(Choice::ConnectMidi(1))));
	assert!(app.modal.is_none());

	app.open_dialog(midi_dialog());
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None);
	assert!(app.modal.is_none());
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
fn f9_requests_panic_and_the_midi_file_has_its_own_keys() {
	let mut app = App::new("en");
	assert_eq!(app.handle_key(plain(KeyCode::F(9))), Some(Request::Panic));
	// Space no longer silences everything: it plays and pauses the file (spec v1.2, decision ОР-3).
	assert_eq!(app.handle_key(plain(KeyCode::Char(' '))), Some(Request::FilePlayPause));
	assert_eq!(app.handle_key(plain(KeyCode::Backspace)), Some(Request::FileStop));
	assert_eq!(app.handle_key(plain(KeyCode::F(11))), Some(Request::OpenMidiFileDialog));
	// In a dialog these keys belong to the dialog: nothing is asked of the file.
	app.open_dialog(midi_dialog());
	assert_eq!(app.handle_key(plain(KeyCode::Char(' '))), None);
	assert_eq!(app.handle_key(plain(KeyCode::Backspace)), None);
	assert_eq!(app.handle_key(plain(KeyCode::F(11))), None);
	assert!(app.modal.is_some());
}

#[test]
fn f1_opens_help_which_scrolls_and_closes() {
	let mut app = App::new("en");
	assert_eq!(app.handle_key(plain(KeyCode::F(1))), None);
	assert_eq!(app.modal, Some(Modal::Help(0)));
	app.handle_key(plain(KeyCode::Down));
	app.handle_key(plain(KeyCode::PageDown));
	assert_eq!(app.modal, Some(Modal::Help(11)));
	app.handle_key(plain(KeyCode::Up));
	assert_eq!(app.modal, Some(Modal::Help(10)));
	assert_eq!(app.handle_key(plain(KeyCode::F(3))), None, "hotkeys do not leak through the help");
	app.handle_key(plain(KeyCode::F(1)));
	assert!(app.modal.is_none());
	app.handle_key(plain(KeyCode::F(1)));
	app.handle_key(plain(KeyCode::Esc));
	assert!(app.modal.is_none());
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
fn notes_are_not_played_while_a_modal_is_open() {
	let mut app = App::new("en");
	app.open_dialog(midi_dialog());
	assert_eq!(app.handle_key(plain(KeyCode::Char('z'))), None);
	assert_eq!(
		app.handle_release(press_kind(KeyCode::Char('z'), KeyModifiers::NONE, KeyEventKind::Release)),
		None
	);
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
fn f6_dialog_applies_changes_live_saves_on_enter_and_rolls_back_on_esc() {
	use crate::session::EffectsAction;
	use crate::ui::effects_dialog::EffectsDialog;
	let settings = EffectsSettings {
		reverb: true,
		chorus: false,
		reverb_level: 40,
		chorus_level: 20,
	};
	let mut app = App::new("en");
	app.open_modal(Modal::Effects(EffectsDialog::new(settings)));
	assert_eq!(app.handle_key(plain(KeyCode::F(3))), None, "hotkeys do not leak through the dialog");
	let toggled = EffectsSettings { reverb: false, ..settings };
	assert_eq!(
		app.handle_key(plain(KeyCode::Char(' '))),
		Some(Request::Effects(EffectsAction::Set(toggled)))
	);
	assert!(app.modal.is_some(), "still open after a change");
	assert_eq!(app.handle_key(plain(KeyCode::Enter)), Some(Request::SaveEffects(toggled)));
	assert!(app.modal.is_none());

	app.open_modal(Modal::Effects(EffectsDialog::new(settings)));
	app.handle_key(plain(KeyCode::Right));
	assert_eq!(
		app.handle_key(plain(KeyCode::Esc)),
		Some(Request::Effects(EffectsAction::Set(settings))),
		"Esc restores the opening state"
	);
	assert!(app.modal.is_none());
	app.open_modal(Modal::Effects(EffectsDialog::new(settings)));
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None, "nothing to roll back");
}

#[test]
fn home_and_end_drive_recording_and_quit_asks_for_confirmation_while_recording() {
	let mut app = App::new("en");
	assert_eq!(app.handle_key(plain(KeyCode::Home)), Some(Request::StartRecord));
	assert_eq!(app.handle_key(plain(KeyCode::End)), Some(Request::StopRecord));
	app.recording = Some(Duration::from_secs(12));
	assert_eq!(app.handle_key(plain(KeyCode::F(10))), None);
	assert_eq!(app.modal, Some(Modal::ConfirmQuit));
	assert!(!app.should_quit);
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None, "Esc keeps recording");
	assert!(app.modal.is_none() && !app.should_quit);
	app.handle_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL));
	assert_eq!(app.handle_key(plain(KeyCode::Enter)), Some(Request::StopRecordingAndQuit));
	assert!(app.should_quit);
	let mut idle = App::new("en");
	idle.handle_key(plain(KeyCode::F(10)));
	assert!(idle.should_quit, "no confirmation when not recording");
}

#[test]
fn every_available_button_gives_the_command_of_its_hotkey() {
	let cases = [
		(ButtonId::MidiIn, Request::OpenDialog(DialogKind::MidiInput)),
		(ButtonId::Sf2File, Request::OpenSf2Dialog),
		(ButtonId::Preset, Request::OpenDialog(DialogKind::Preset)),
		(ButtonId::Effects, Request::OpenEffectsDialog),
		(ButtonId::AudioOut, Request::OpenDialog(DialogKind::AudioOutput)),
		(ButtonId::MasterDown, Request::Mixer(MixerAction::MasterDownFast)),
		(ButtonId::MasterUp, Request::Mixer(MixerAction::MasterUpFast)),
		(ButtonId::Folder, Request::OpenRecordDialog),
		(ButtonId::FormatFlac, Request::SetRecordFormat(RecordFormat::Flac)),
		(ButtonId::FormatOgg, Request::SetRecordFormat(RecordFormat::OggVorbis)),
		(ButtonId::FormatMidi, Request::SetRecordFormat(RecordFormat::Midi)),
		(ButtonId::RecordStart, Request::StartRecord),
		(ButtonId::RecordStop, Request::StopRecord),
		(ButtonId::RecordCancel, Request::CancelRecord),
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
fn the_format_cannot_be_changed_under_a_running_take() {
	let mut app = App::new("en");
	app.recording = Some(Duration::from_secs(1));
	assert_eq!(app.press_button(ButtonId::FormatOgg), None);
	assert_eq!(app.press_button(ButtonId::FormatFlac), None);
	assert_eq!(app.press_button(ButtonId::FormatMidi), None);
	app.recording = None;
	assert_eq!(
		app.press_button(ButtonId::FormatOgg),
		Some(Request::SetRecordFormat(RecordFormat::OggVorbis))
	);
}

#[test]
fn dismissing_a_dialog_does_what_esc_does_in_it() {
	let mut app = App::new("en");
	assert_eq!(app.dismiss_modal(), None, "nothing is open");
	app.take_dirty();
	app.open_dialog(midi_dialog());
	assert_eq!(app.dismiss_modal(), None);
	assert!(app.modal.is_none() && app.take_dirty());

	// The effects dialog applies changes as they are made: leaving it this way rolls them back.
	let before = EffectsSettings {
		reverb: true,
		reverb_level: 40,
		chorus: false,
		chorus_level: 20,
	};
	app.open_modal(Modal::Effects(EffectsDialog::new(before)));
	app.handle_key(plain(KeyCode::Char(' ')));
	assert_eq!(app.dismiss_modal(), Some(Request::Effects(EffectsAction::Set(before))));
	assert!(app.modal.is_none());

	// Esc bound to something else changes nothing: a click outside is not a key.
	app.bindings = Bindings::from_config(
		&[("notes_off", "Ctrl+F2"), ("quit", "Esc")]
			.into_iter()
			.map(|(command, chord)| (command.to_owned(), chord.to_owned()))
			.collect(),
		&KeyboardConfig::default(),
	)
	.expect("valid bindings");
	app.modal = Some(Modal::Help(0));
	assert_eq!(app.dismiss_modal(), None);
	assert!(app.modal.is_none() && !app.should_quit);
}
