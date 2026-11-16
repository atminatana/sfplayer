//! Tests of the key translation: physical place for commands, layout character for text.

use winit::keyboard::SmolStr;

use super::*;
use crate::config::KeyboardConfig;
use crate::ui::text_keyboard::TextKeyboard;

fn character(text: &str) -> Key {
	Key::Character(SmolStr::new(text))
}

/// A press of the physical key `code` that the current layout turned into `logical`.
fn key<'a>(code: PhysicalCode, logical: &'a Key, held: ModifiersState) -> WindowKey<'a> {
	WindowKey {
		physical: PhysicalKey::Code(code),
		logical,
		state: ElementState::Pressed,
		is_repeat: false,
		held,
	}
}

fn command(code: PhysicalCode, logical: &Key, held: ModifiersState) -> Option<KeyEvent> {
	key_from_window(&key(code, logical, held), Reading::Command)
}

fn text(code: PhysicalCode, logical: &Key, held: ModifiersState) -> Option<KeyEvent> {
	key_from_window(&key(code, logical, held), Reading::Text)
}

const NONE: ModifiersState = ModifiersState::empty();

#[test]
fn named_keys_translate_to_the_ui_key_codes_in_both_readings() {
	for (named, physical, code) in [
		(NamedKey::ArrowUp, PhysicalCode::ArrowUp, KeyCode::Up),
		(NamedKey::ArrowDown, PhysicalCode::ArrowDown, KeyCode::Down),
		(NamedKey::ArrowLeft, PhysicalCode::ArrowLeft, KeyCode::Left),
		(NamedKey::ArrowRight, PhysicalCode::ArrowRight, KeyCode::Right),
		(NamedKey::Home, PhysicalCode::Home, KeyCode::Home),
		(NamedKey::End, PhysicalCode::End, KeyCode::End),
		(NamedKey::PageUp, PhysicalCode::PageUp, KeyCode::PageUp),
		(NamedKey::PageDown, PhysicalCode::PageDown, KeyCode::PageDown),
		(NamedKey::Enter, PhysicalCode::Enter, KeyCode::Enter),
		(NamedKey::Escape, PhysicalCode::Escape, KeyCode::Esc),
		(NamedKey::Tab, PhysicalCode::Tab, KeyCode::Tab),
		(NamedKey::Backspace, PhysicalCode::Backspace, KeyCode::Backspace),
		(NamedKey::Delete, PhysicalCode::Delete, KeyCode::Delete),
		(NamedKey::Insert, PhysicalCode::Insert, KeyCode::Insert),
		(NamedKey::Space, PhysicalCode::Space, KeyCode::Char(' ')),
		(NamedKey::F1, PhysicalCode::F1, KeyCode::F(1)),
		(NamedKey::F10, PhysicalCode::F10, KeyCode::F(10)),
		(NamedKey::F12, PhysicalCode::F12, KeyCode::F(12)),
		(NamedKey::F24, PhysicalCode::F24, KeyCode::F(24)),
	] {
		let logical = Key::Named(named);
		let expected = Some(KeyEvent::new(code, KeyModifiers::NONE));
		assert_eq!(command(physical, &logical, NONE), expected, "{named:?} as a command");
		assert_eq!(text(physical, &logical, NONE), expected, "{named:?} as text");
	}
	// Keys the UI does not distinguish never reach the handlers.
	for (named, physical) in [
		(NamedKey::CapsLock, PhysicalCode::CapsLock),
		(NamedKey::Shift, PhysicalCode::ShiftLeft),
		(NamedKey::Control, PhysicalCode::ControlLeft),
		(NamedKey::F25, PhysicalCode::F25),
		(NamedKey::PrintScreen, PhysicalCode::PrintScreen),
	] {
		assert_eq!(command(physical, &Key::Named(named), NONE), None, "{named:?}");
	}
}

#[test]
fn the_numeric_pad_follows_num_lock() {
	// Num Lock off: the layout reports a navigation key, and that is what the UI gets.
	assert_eq!(
		command(PhysicalCode::Numpad8, &Key::Named(NamedKey::ArrowUp), NONE),
		Some(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
	);
	// Num Lock on: a digit; the operator keys give their characters whatever Shift does.
	assert_eq!(
		command(PhysicalCode::Numpad8, &character("8"), NONE),
		Some(KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE))
	);
	assert_eq!(
		command(PhysicalCode::NumpadAdd, &character("+"), NONE),
		Some(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
	);
	assert_eq!(
		command(PhysicalCode::NumpadSubtract, &character("-"), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::SHIFT))
	);
}

#[test]
fn modifiers_and_event_kinds_are_carried_over() {
	assert_eq!(
		command(
			PhysicalCode::ArrowLeft,
			&Key::Named(NamedKey::ArrowLeft),
			ModifiersState::CONTROL | ModifiersState::ALT
		),
		Some(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::ALT))
	);
	assert_eq!(
		command(PhysicalCode::F1, &Key::Named(NamedKey::F1), ModifiersState::SUPER),
		Some(KeyEvent::new(KeyCode::F(1), KeyModifiers::SUPER))
	);
	// Shift+Tab arrives as the back-tab key.
	assert_eq!(
		command(PhysicalCode::Tab, &Key::Named(NamedKey::Tab), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
	);

	let logical = character("z");
	let kind_of = |state, is_repeat| {
		let key = WindowKey {
			physical: PhysicalKey::Code(PhysicalCode::KeyZ),
			logical: &logical,
			state,
			is_repeat,
			held: NONE,
		};
		key_from_window(&key, Reading::Command).map(|key| key.kind)
	};
	assert_eq!(kind_of(ElementState::Pressed, false), Some(KeyEventKind::Press));
	assert_eq!(kind_of(ElementState::Pressed, true), Some(KeyEventKind::Repeat));
	assert_eq!(kind_of(ElementState::Released, false), Some(KeyEventKind::Release));
	assert_eq!(
		kind_of(ElementState::Released, true),
		Some(KeyEventKind::Release),
		"a release that still carries the repeat flag is a release"
	);
}

#[test]
fn a_command_is_read_by_the_place_of_the_key_whatever_the_layout() {
	// The key that is Z on a US keyboard, typed in a Russian and in a German layout.
	for logical in [character("z"), character("я"), character("y")] {
		assert_eq!(
			command(PhysicalCode::KeyZ, &logical, NONE),
			Some(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
			"{logical:?}"
		);
	}
	// Shift gives what Shift gives on a US keyboard.
	assert_eq!(
		command(PhysicalCode::KeyZ, &character("Я"), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT))
	);
	assert_eq!(
		command(PhysicalCode::Equal, &character("="), NONE),
		Some(KeyEvent::new(KeyCode::Char('='), KeyModifiers::NONE))
	);
	assert_eq!(
		command(PhysicalCode::Equal, &character("+"), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::SHIFT))
	);
	assert_eq!(
		command(PhysicalCode::Minus, &character("_"), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('_'), KeyModifiers::SHIFT))
	);
	// The comma key carries «б» in the Russian layout; a dead key carries no character at all.
	assert_eq!(
		command(PhysicalCode::Comma, &character("б"), NONE),
		Some(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE))
	);
	assert_eq!(
		command(PhysicalCode::Backquote, &Key::Dead(Some('`')), NONE),
		Some(KeyEvent::new(KeyCode::Char('`'), KeyModifiers::NONE))
	);
	// A key with no place on a US keyboard keeps the character of its layout.
	assert_eq!(
		command(PhysicalCode::IntlBackslash, &character("<"), NONE),
		Some(KeyEvent::new(KeyCode::Char('<'), KeyModifiers::NONE))
	);
	assert_eq!(command(PhysicalCode::IntlBackslash, &Key::Dead(None), NONE), None);
}

#[test]
fn text_is_read_as_the_layout_produced_it() {
	assert_eq!(
		text(PhysicalCode::KeyZ, &character("я"), NONE),
		Some(KeyEvent::new(KeyCode::Char('я'), KeyModifiers::NONE))
	);
	assert_eq!(
		text(PhysicalCode::KeyZ, &character("Я"), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('Я'), KeyModifiers::SHIFT))
	);
	assert_eq!(
		text(PhysicalCode::Digit2, &character("\""), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('"'), KeyModifiers::SHIFT)),
		"the Russian layout has a quote on Shift+2"
	);
	// A key that produced more than one character, or none, is not a single key press for the UI.
	assert_eq!(text(PhysicalCode::KeyA, &character("ab"), NONE), None);
	assert_eq!(text(PhysicalCode::KeyA, &character(""), NONE), None);
	assert_eq!(text(PhysicalCode::Backquote, &Key::Dead(Some('`')), NONE), None);
}

#[test]
fn hotkeys_work_in_a_russian_layout() {
	let bindings = Bindings::default();
	let action = |code, logical: &str, held| command(code, &character(logical), held).and_then(|key| bindings.action_for(&key));
	assert_eq!(action(PhysicalCode::KeyU, "г", ModifiersState::CONTROL), Some(Action::MasterMute));
	assert_eq!(action(PhysicalCode::KeyR, "к", ModifiersState::ALT), Some(Action::ToggleReverb));
	assert_eq!(action(PhysicalCode::KeyC, "с", ModifiersState::ALT), Some(Action::ToggleChorus));
	assert_eq!(action(PhysicalCode::BracketLeft, "х", NONE), Some(Action::TransposeDown));
	assert_eq!(action(PhysicalCode::BracketRight, "ъ", NONE), Some(Action::TransposeUp));
	// The volume keys: `-` and `=` plain, `+` and `_` with Shift, and the numeric pad.
	assert_eq!(action(PhysicalCode::Minus, "-", NONE), Some(Action::MasterDown));
	assert_eq!(action(PhysicalCode::Equal, "=", NONE), Some(Action::MasterUp));
	assert_eq!(action(PhysicalCode::Equal, "+", ModifiersState::SHIFT), Some(Action::MasterUpFast));
	assert_eq!(
		action(PhysicalCode::Minus, "_", ModifiersState::SHIFT),
		Some(Action::MasterDownFast)
	);
	assert_eq!(action(PhysicalCode::NumpadAdd, "+", NONE), Some(Action::MasterUp));
	assert_eq!(action(PhysicalCode::NumpadSubtract, "-", NONE), Some(Action::MasterDown));
	// A plain letter is no command: it is left for the text keyboard.
	assert_eq!(action(PhysicalCode::KeyZ, "я", NONE), None);

	let named = |named, physical| command(physical, &Key::Named(named), NONE).and_then(|key| bindings.action_for(&key));
	assert_eq!(named(NamedKey::F1, PhysicalCode::F1), Some(Action::Help));
	assert_eq!(named(NamedKey::F10, PhysicalCode::F10), Some(Action::Quit));
	assert_eq!(named(NamedKey::F9, PhysicalCode::F9), Some(Action::Panic));
	assert_eq!(named(NamedKey::Escape, PhysicalCode::Escape), Some(Action::NotesOff));
}

#[test]
fn the_note_rows_play_in_a_russian_layout() {
	let keyboard = TextKeyboard::new(&KeyboardConfig::default());
	let note = |code, logical: &str| {
		let key = command(code, &character(logical), NONE).expect("a character key");
		let KeyCode::Char(character) = key.code else {
			panic!("not a character key: {key:?}");
		};
		keyboard.note_for(character)
	};
	// Bottom row Z X C … / is «я ч с … .» in the Russian layout; the row above carries the black keys.
	assert_eq!(note(PhysicalCode::KeyZ, "я"), Some(60));
	assert_eq!(note(PhysicalCode::KeyX, "ч"), Some(62));
	assert_eq!(note(PhysicalCode::KeyS, "ы"), Some(61));
	assert_eq!(note(PhysicalCode::Comma, "б"), Some(72));
	assert_eq!(note(PhysicalCode::Period, "ю"), Some(74));
	assert_eq!(note(PhysicalCode::Slash, "."), Some(76));
	assert_eq!(note(PhysicalCode::Semicolon, "ж"), Some(75));
	assert_eq!(note(PhysicalCode::KeyQ, "й"), None, "the upper rows are not note keys");
}

#[test]
fn in_a_dialog_plain_characters_are_text_and_chords_stay_commands() {
	let reading = |code, modifiers| reading_in_a_dialog(&KeyEvent::new(code, modifiers));
	assert_eq!(reading(KeyCode::Char('z'), KeyModifiers::NONE), Reading::Text);
	assert_eq!(reading(KeyCode::Char('Z'), KeyModifiers::SHIFT), Reading::Text);
	assert_eq!(reading(KeyCode::Char('/'), KeyModifiers::NONE), Reading::Text);
	// Chords, named keys and the space bar (a switch in the effects dialog) are commands.
	assert_eq!(reading(KeyCode::Char('c'), KeyModifiers::CONTROL), Reading::Command);
	assert_eq!(reading(KeyCode::Char('r'), KeyModifiers::ALT), Reading::Command);
	assert_eq!(reading(KeyCode::Char(' '), KeyModifiers::NONE), Reading::Command);
	assert_eq!(reading(KeyCode::Enter, KeyModifiers::NONE), Reading::Command);
	assert_eq!(reading(KeyCode::F(10), KeyModifiers::NONE), Reading::Command);
}

#[test]
fn in_a_text_field_only_non_character_quit_keys_quit() {
	// Quit is on F10; a config may put it on Ctrl+C as well (a default before spec v1.3).
	let keys = std::collections::BTreeMap::from([("quit".to_owned(), "F10, Ctrl+C".to_owned())]);
	let bindings = Bindings::from_config(&keys, &KeyboardConfig::default()).expect("valid");
	assert!(quits_from_a_field(&bindings, &KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE)));
	// Ctrl+C is a quit chord then, but in a field it copies.
	assert!(!quits_from_a_field(
		&bindings,
		&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
	));
	assert!(!quits_from_a_field(&bindings, &KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));
	assert!(!quits_from_a_field(&bindings, &KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
	let mut release = KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE);
	release.kind = KeyEventKind::Release;
	assert!(!quits_from_a_field(&bindings, &release));
}
