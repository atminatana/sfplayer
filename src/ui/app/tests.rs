//! Key handling and request tests for `App` (no devices involved).

use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

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
}

#[test]
fn f7_dialog_takes_paste_into_the_folder_and_reports_folder_and_format_on_enter() {
	use crate::config::RecordFormat;
	use crate::ui::record_dialog::RecordDialog;
	let mut app = App::new("en");
	app.open_modal(Modal::Record(RecordDialog::new(None, RecordFormat::Flac, 0.5)));
	app.handle_paste("D:/takes");
	assert_eq!(
		app.handle_key(plain(KeyCode::Home)),
		None,
		"Home edits the field, no recording starts"
	);
	app.handle_key(plain(KeyCode::Down));
	app.handle_key(plain(KeyCode::Right));
	assert_eq!(
		app.handle_key(plain(KeyCode::Enter)),
		Some(Request::Apply(Choice::SetRecording {
			dir: Some(PathBuf::from("D:/takes")),
			format: RecordFormat::OggVorbis
		}))
	);
	assert!(app.modal.is_none());
	app.open_modal(Modal::Record(RecordDialog::new(None, RecordFormat::Flac, 0.5)));
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None);
	assert!(app.modal.is_none());
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
fn sf2_dialog_submits_a_path_and_stays_open_until_fulfilled() {
	let mut app = App::new("en");
	app.open_modal(Modal::Sf2Path(PathInput::new("", vec![])));
	app.handle_paste("assets/sin_wave.sf2");
	assert_eq!(
		app.handle_key(plain(KeyCode::Enter)),
		Some(Request::LoadBank(PathBuf::from("assets/sin_wave.sf2")))
	);
	assert!(app.modal.is_some(), "closed only by fulfil on success");
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None);
	assert!(app.modal.is_none());
}

#[test]
fn paste_outside_the_path_dialogs_is_ignored() {
	let mut app = App::new("en");
	app.take_dirty();
	app.handle_paste("ignored");
	assert!(!app.take_dirty());
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
fn f9_and_space_request_panic() {
	let mut app = App::new("en");
	assert_eq!(app.handle_key(plain(KeyCode::F(9))), Some(Request::Panic));
	assert_eq!(app.handle_key(plain(KeyCode::Char(' '))), Some(Request::Panic));
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

fn release_app() -> App {
	use crate::ui::text_keyboard::{NoteOffMode, TextKeyboard};
	App::with_keyboard(
		"en",
		TextKeyboard::new(&crate::config::KeyboardConfig::default(), NoteOffMode::Release),
	)
}

#[test]
fn text_keyboard_plays_notes_on_channel_one_and_stops_on_release_or_esc() {
	let mut app = release_app();
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
		None,
		"the note-off waits out the debounce window"
	);
	assert_eq!(
		app.tick_keyboard(Instant::now() + Duration::from_millis(50)),
		Some(Request::Notes(vec![MidiEvent::NoteOff { channel: 0, key: 60 }])),
		"the tick releases it once the window closes"
	);
	assert_eq!(
		app.handle_key(plain(KeyCode::Esc)),
		Some(Request::Notes(vec![MidiEvent::NoteOff { channel: 0, key: 61 }]))
	);
	assert_eq!(app.handle_key(plain(KeyCode::Esc)), None, "nothing left to release");
}

#[test]
fn ctrl_arrows_change_octave_and_velocity_and_panic_forgets_held_notes() {
	let mut app = release_app();
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
fn gate_mode_releases_through_the_tick() {
	let mut app = App::new("en");
	app.handle_key(plain(KeyCode::Char('z')));
	assert_eq!(
		app.handle_release(press_kind(KeyCode::Char('z'), KeyModifiers::NONE, KeyEventKind::Release)),
		None,
		"gate mode ignores releases"
	);
	assert_eq!(app.tick_keyboard(Instant::now()), None);
	assert_eq!(
		app.tick_keyboard(Instant::now() + Duration::from_secs(1)),
		Some(Request::Notes(vec![MidiEvent::NoteOff { channel: 0, key: 60 }]))
	);
}

#[test]
fn notes_are_not_played_while_a_modal_is_open() {
	let mut app = release_app();
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
