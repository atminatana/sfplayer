//! Key handling and request tests for `App` (no devices involved).

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::*;
use crate::midi::MidiPortInfo;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
	KeyEvent::new(code, modifiers)
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
fn tab_cycles_focus_between_the_two_panels() {
	let mut app = App::new("en");
	assert_eq!(app.focus, Panel::Monitor);
	assert_eq!(app.handle_key(plain(KeyCode::Tab)), None);
	assert_eq!(app.focus, Panel::Mixer);
	assert_eq!(app.handle_key(press(KeyCode::BackTab, KeyModifiers::SHIFT)), None);
	assert_eq!(app.focus, Panel::Monitor);
	assert!(app.take_dirty());
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
	assert_eq!(app.handle_key(plain(KeyCode::Char('z'))), None);
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
	assert_eq!(
		app.handle_key(plain(KeyCode::F(6))),
		Some(Request::OpenDialog(DialogKind::RecordFormat))
	);
	assert_eq!(app.handle_key(plain(KeyCode::F(8))), Some(Request::Rescan));
}

#[test]
fn dialog_captures_keys_until_enter_or_esc() {
	let mut app = App::new("en");
	app.open_dialog(midi_dialog());
	assert_eq!(app.handle_key(plain(KeyCode::Tab)), None, "Tab goes to the dialog, not to focus");
	assert_eq!(app.focus, Panel::Monitor);
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
fn paste_outside_the_path_dialog_is_ignored() {
	let mut app = App::new("en");
	app.take_dirty();
	app.handle_paste("ignored");
	assert!(!app.take_dirty());
}

#[test]
fn monitor_keys_work_only_while_the_monitor_panel_is_focused() {
	use crate::midi::{MidiEvent, MonitorEntry};
	let mut app = App::new("en");
	app.monitor
		.extend((0..5).map(|key| MonitorEntry::now(MidiEvent::NoteOff { channel: 0, key })));
	app.handle_key(plain(KeyCode::Up));
	assert_eq!(app.monitor.scroll(), 1);
	app.handle_key(plain(KeyCode::PageUp));
	assert_eq!(app.monitor.scroll(), 4, "clamped to the oldest entry");
	app.handle_key(plain(KeyCode::End));
	assert_eq!(app.monitor.scroll(), 0);
	app.handle_key(press(KeyCode::Char('f'), KeyModifiers::CONTROL));
	assert!(app.monitor.is_paused());
	app.handle_key(plain(KeyCode::Tab));
	app.handle_key(press(KeyCode::Char('l'), KeyModifiers::CONTROL));
	assert_eq!(app.monitor.len(), 5, "Ctrl+L ignored while the mixer is focused");
	app.handle_key(plain(KeyCode::Tab));
	app.handle_key(press(KeyCode::Char('l'), KeyModifiers::CONTROL));
	assert!(app.monitor.is_empty());
}

#[test]
fn master_keys_work_everywhere_but_channel_keys_need_the_mixer_focus() {
	let mut app = App::new("en");
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('+'))),
		Some(Request::Mixer(MixerAction::MasterUp))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('u'), KeyModifiers::CONTROL)),
		Some(Request::Mixer(MixerAction::MasterMute))
	);
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('3'))),
		None,
		"monitor focused: digits do nothing"
	);
	assert_eq!(app.handle_key(plain(KeyCode::Right)), None);
	app.handle_key(plain(KeyCode::Tab));
	assert_eq!(
		app.handle_key(plain(KeyCode::Char('3'))),
		Some(Request::Mixer(MixerAction::SelectChannel(2)))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('2'), KeyModifiers::ALT)),
		Some(Request::Mixer(MixerAction::SelectChannel(11)))
	);
	assert_eq!(app.handle_key(plain(KeyCode::Right)), Some(Request::Mixer(MixerAction::ChannelUp)));
	assert_eq!(app.handle_key(plain(KeyCode::Left)), Some(Request::Mixer(MixerAction::ChannelDown)));
	assert_eq!(
		app.handle_key(press(KeyCode::Char('x'), KeyModifiers::CONTROL)),
		Some(Request::Mixer(MixerAction::ChannelMute))
	);
	assert_eq!(
		app.handle_key(press(KeyCode::Char('o'), KeyModifiers::CONTROL)),
		Some(Request::Mixer(MixerAction::ChannelSolo))
	);
	assert_eq!(
		app.handle_key(plain(KeyCode::Up)),
		None,
		"monitor scrolling is inactive while the mixer is focused"
	);
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
