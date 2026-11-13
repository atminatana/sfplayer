//! Tests of the window glue that need no window: key translation and the "what the frame shows" key.

use winit::keyboard::SmolStr;

use super::*;
use crate::audio::{Levels, Mixer};
use crate::config::RecordFormat;
use crate::midi::MidiStatus;
use crate::ui::keys::{Action, Bindings};

fn character(text: &str) -> Key {
	Key::Character(SmolStr::new(text))
}

fn pressed(key: &Key, held: ModifiersState) -> Option<KeyEvent> {
	key_from_window(key, ElementState::Pressed, false, held)
}

fn view() -> View {
	View {
		midi: MidiStatus::NoPort,
		audio: None,
		bank: None,
		reverb: None,
		chorus: None,
		record_dir: String::new(),
		record_format: RecordFormat::Flac,
		xruns: 0,
		dropped: 0,
		transpose: 0,
		omni_channel: None,
		record_dropped: 0,
		held_notes: 0,
		max_voices: 64,
		load_percent: 0,
		levels: Levels::default(),
		mixer: Mixer::new(80),
		notice: None,
	}
}

#[test]
fn named_keys_translate_to_the_ui_key_codes() {
	let none = ModifiersState::empty();
	for (named, code) in [
		(NamedKey::ArrowUp, KeyCode::Up),
		(NamedKey::ArrowDown, KeyCode::Down),
		(NamedKey::ArrowLeft, KeyCode::Left),
		(NamedKey::ArrowRight, KeyCode::Right),
		(NamedKey::Home, KeyCode::Home),
		(NamedKey::End, KeyCode::End),
		(NamedKey::PageUp, KeyCode::PageUp),
		(NamedKey::PageDown, KeyCode::PageDown),
		(NamedKey::Enter, KeyCode::Enter),
		(NamedKey::Escape, KeyCode::Esc),
		(NamedKey::Tab, KeyCode::Tab),
		(NamedKey::Backspace, KeyCode::Backspace),
		(NamedKey::Delete, KeyCode::Delete),
		(NamedKey::Insert, KeyCode::Insert),
		(NamedKey::Space, KeyCode::Char(' ')),
		(NamedKey::F1, KeyCode::F(1)),
		(NamedKey::F10, KeyCode::F(10)),
		(NamedKey::F12, KeyCode::F(12)),
		(NamedKey::F24, KeyCode::F(24)),
	] {
		assert_eq!(
			pressed(&Key::Named(named), none),
			Some(KeyEvent::new(code, KeyModifiers::NONE)),
			"{named:?}"
		);
	}
	// Keys the UI does not distinguish never reach the handlers.
	for named in [
		NamedKey::CapsLock,
		NamedKey::Shift,
		NamedKey::Control,
		NamedKey::F25,
		NamedKey::PrintScreen,
	] {
		assert_eq!(pressed(&Key::Named(named), none), None, "{named:?}");
	}
	assert_eq!(pressed(&Key::Dead(Some('`')), none), None);
}

#[test]
fn modifiers_and_event_kinds_are_carried_over() {
	assert_eq!(
		pressed(&character("c"), ModifiersState::CONTROL),
		Some(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
	);
	assert_eq!(
		pressed(&Key::Named(NamedKey::ArrowLeft), ModifiersState::CONTROL | ModifiersState::ALT),
		Some(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::ALT))
	);
	assert_eq!(
		pressed(&Key::Named(NamedKey::F1), ModifiersState::SUPER),
		Some(KeyEvent::new(KeyCode::F(1), KeyModifiers::SUPER))
	);
	// Shift+Tab arrives as the back-tab key, as terminals report it.
	assert_eq!(
		pressed(&Key::Named(NamedKey::Tab), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
	);

	let repeat = key_from_window(&character("z"), ElementState::Pressed, true, ModifiersState::empty());
	assert_eq!(repeat.map(|key| key.kind), Some(KeyEventKind::Repeat));
	let release = key_from_window(&character("z"), ElementState::Released, false, ModifiersState::empty());
	assert_eq!(release.map(|key| key.kind), Some(KeyEventKind::Release));
	// A release that still carries the repeat flag is a release.
	let late_release = key_from_window(&character("z"), ElementState::Released, true, ModifiersState::empty());
	assert_eq!(late_release.map(|key| key.kind), Some(KeyEventKind::Release));
}

#[test]
fn character_keys_keep_the_character_the_layout_produced() {
	let none = ModifiersState::empty();
	assert_eq!(
		pressed(&character("z"), none),
		Some(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE))
	);
	assert_eq!(
		pressed(&character("Z"), ModifiersState::SHIFT),
		Some(KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT))
	);
	assert_eq!(
		pressed(&character("я"), none),
		Some(KeyEvent::new(KeyCode::Char('я'), KeyModifiers::NONE))
	);
	// A key that produced more than one character, or none, is not a single key press for the UI.
	assert_eq!(pressed(&character("ab"), none), None);
	assert_eq!(pressed(&character(""), none), None);
}

#[test]
fn window_chords_reach_the_same_commands_as_in_the_terminal() {
	let bindings = Bindings::default();
	let action = |key: &Key, held: ModifiersState| pressed(key, held).and_then(|key| bindings.action_for(&key));
	let none = ModifiersState::empty();
	assert_eq!(action(&Key::Named(NamedKey::F1), none), Some(Action::Help));
	assert_eq!(action(&Key::Named(NamedKey::F10), none), Some(Action::Quit));
	assert_eq!(action(&character("c"), ModifiersState::CONTROL), Some(Action::Quit));
	assert_eq!(action(&Key::Named(NamedKey::Space), none), Some(Action::Panic));
	assert_eq!(action(&Key::Named(NamedKey::Home), none), Some(Action::RecordStart));
	assert_eq!(
		action(&Key::Named(NamedKey::ArrowUp), ModifiersState::CONTROL),
		Some(Action::OctaveUp)
	);
	assert_eq!(action(&character("r"), ModifiersState::ALT), Some(Action::ToggleReverb));
	assert_eq!(action(&character("u"), ModifiersState::CONTROL), Some(Action::MasterMute));
	assert_eq!(action(&character("]"), none), Some(Action::TransposeUp));
	// The volume keys as a keyboard produces them: `-` and `=` plain, `+` and `_` with Shift held.
	assert_eq!(action(&character("-"), none), Some(Action::MasterDown));
	assert_eq!(action(&character("="), none), Some(Action::MasterUp));
	assert_eq!(action(&character("+"), ModifiersState::SHIFT), Some(Action::MasterUpFast));
	assert_eq!(action(&character("_"), ModifiersState::SHIFT), Some(Action::MasterDownFast));
	// A plain letter is no command: it is left for the text keyboard.
	assert_eq!(action(&character("z"), none), None);
}

#[test]
fn the_frame_key_changes_only_when_the_picture_would() {
	let grid = (96, 31);
	let app = App::new("en");
	let shown = FrameState::new(view(), &app, grid);
	assert_eq!(FrameState::new(view(), &app, grid), shown, "nothing changed: no redraw");

	let mut louder = view();
	louder.levels.rms_left = 0.25;
	assert_ne!(FrameState::new(louder, &app, grid), shown, "a meter moved");
	assert_ne!(FrameState::new(view(), &app, (120, 40)), shown, "the grid changed");

	let mut clipping = App::new("en");
	clipping.clipping = true;
	assert_ne!(FrameState::new(view(), &clipping, grid), shown);

	let mut higher = App::new("en");
	higher.keyboard.octave_up();
	assert_ne!(FrameState::new(view(), &higher, grid), shown);
	let mut softer = App::new("en");
	softer.keyboard.velocity_down();
	assert_ne!(FrameState::new(view(), &softer, grid), shown);
}

#[test]
fn a_running_take_redraws_once_per_blink_phase_and_per_second() {
	let grid = (96, 31);
	let at = |milliseconds: u64| {
		let mut app = App::new("en");
		app.recording = Some(Duration::from_millis(milliseconds));
		FrameState::new(view(), &app, grid)
	};
	assert_ne!(at(0), FrameState::new(view(), &App::new("en"), grid), "the indicator appears");
	assert_eq!(at(0), at(499), "same second, dot still lit: no redraw");
	assert_ne!(at(499), at(500), "the dot goes dark");
	assert_eq!(at(500), at(999));
	assert_ne!(at(999), at(1_000), "the next second starts and the dot lights up");
	assert!(blink_phase(Duration::from_millis(0)));
	assert!(!blink_phase(Duration::from_millis(500)));
	assert!(blink_phase(Duration::from_millis(1_000)));
}

#[test]
fn paste_is_ctrl_v_or_shift_insert_and_nothing_else() {
	let chord = |code, modifiers| KeyEvent::new(code, modifiers);
	assert!(is_paste_chord(&chord(KeyCode::Char('v'), KeyModifiers::CONTROL)));
	assert!(is_paste_chord(&chord(KeyCode::Char('V'), KeyModifiers::CONTROL)));
	assert!(is_paste_chord(&chord(KeyCode::Insert, KeyModifiers::SHIFT)));
	// A held chord keeps pasting, as in any text field.
	let mut repeat = chord(KeyCode::Char('v'), KeyModifiers::CONTROL);
	repeat.kind = KeyEventKind::Repeat;
	assert!(is_paste_chord(&repeat));

	assert!(
		!is_paste_chord(&chord(KeyCode::Char('v'), KeyModifiers::NONE)),
		"a plain letter is text"
	);
	assert!(!is_paste_chord(&chord(
		KeyCode::Char('v'),
		KeyModifiers::CONTROL | KeyModifiers::ALT
	)));
	assert!(!is_paste_chord(&chord(KeyCode::Char('c'), KeyModifiers::CONTROL)));
	assert!(!is_paste_chord(&chord(KeyCode::Insert, KeyModifiers::NONE)));
	let mut release = chord(KeyCode::Char('v'), KeyModifiers::CONTROL);
	release.kind = KeyEventKind::Release;
	assert!(!is_paste_chord(&release), "letting go of the chord does not paste again");
}
