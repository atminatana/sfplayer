//! From window key events to the UI's own key type (spec v1.2 §4.3).
//!
//! A character key can be read two ways. As a **command** it is named by its place on the keyboard —
//! the character a US layout has there — so hotkeys and the note keys work whatever language the user
//! types in. As **text** it is the character the current layout produces, which is what a filter or a
//! path field wants. Named keys (arrows, F-keys, Enter…) read the same either way.

use winit::event::ElementState;
use winit::keyboard::{Key, KeyCode as PhysicalCode, ModifiersState, NamedKey, PhysicalKey};

use super::chord::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::keys::{Action, Bindings};

/// Function keys by number: `FUNCTION_KEYS[0]` is F1.
const FUNCTION_KEYS: [NamedKey; 24] = [
	NamedKey::F1,
	NamedKey::F2,
	NamedKey::F3,
	NamedKey::F4,
	NamedKey::F5,
	NamedKey::F6,
	NamedKey::F7,
	NamedKey::F8,
	NamedKey::F9,
	NamedKey::F10,
	NamedKey::F11,
	NamedKey::F12,
	NamedKey::F13,
	NamedKey::F14,
	NamedKey::F15,
	NamedKey::F16,
	NamedKey::F17,
	NamedKey::F18,
	NamedKey::F19,
	NamedKey::F20,
	NamedKey::F21,
	NamedKey::F22,
	NamedKey::F23,
	NamedKey::F24,
];

/// Character keys of a US keyboard: the physical key, its character, its character with Shift.
const US_KEYS: [(PhysicalCode, char, char); 47] = [
	(PhysicalCode::KeyA, 'a', 'A'),
	(PhysicalCode::KeyB, 'b', 'B'),
	(PhysicalCode::KeyC, 'c', 'C'),
	(PhysicalCode::KeyD, 'd', 'D'),
	(PhysicalCode::KeyE, 'e', 'E'),
	(PhysicalCode::KeyF, 'f', 'F'),
	(PhysicalCode::KeyG, 'g', 'G'),
	(PhysicalCode::KeyH, 'h', 'H'),
	(PhysicalCode::KeyI, 'i', 'I'),
	(PhysicalCode::KeyJ, 'j', 'J'),
	(PhysicalCode::KeyK, 'k', 'K'),
	(PhysicalCode::KeyL, 'l', 'L'),
	(PhysicalCode::KeyM, 'm', 'M'),
	(PhysicalCode::KeyN, 'n', 'N'),
	(PhysicalCode::KeyO, 'o', 'O'),
	(PhysicalCode::KeyP, 'p', 'P'),
	(PhysicalCode::KeyQ, 'q', 'Q'),
	(PhysicalCode::KeyR, 'r', 'R'),
	(PhysicalCode::KeyS, 's', 'S'),
	(PhysicalCode::KeyT, 't', 'T'),
	(PhysicalCode::KeyU, 'u', 'U'),
	(PhysicalCode::KeyV, 'v', 'V'),
	(PhysicalCode::KeyW, 'w', 'W'),
	(PhysicalCode::KeyX, 'x', 'X'),
	(PhysicalCode::KeyY, 'y', 'Y'),
	(PhysicalCode::KeyZ, 'z', 'Z'),
	(PhysicalCode::Digit1, '1', '!'),
	(PhysicalCode::Digit2, '2', '@'),
	(PhysicalCode::Digit3, '3', '#'),
	(PhysicalCode::Digit4, '4', '$'),
	(PhysicalCode::Digit5, '5', '%'),
	(PhysicalCode::Digit6, '6', '^'),
	(PhysicalCode::Digit7, '7', '&'),
	(PhysicalCode::Digit8, '8', '*'),
	(PhysicalCode::Digit9, '9', '('),
	(PhysicalCode::Digit0, '0', ')'),
	(PhysicalCode::Minus, '-', '_'),
	(PhysicalCode::Equal, '=', '+'),
	(PhysicalCode::BracketLeft, '[', '{'),
	(PhysicalCode::BracketRight, ']', '}'),
	(PhysicalCode::Backslash, '\\', '|'),
	(PhysicalCode::Semicolon, ';', ':'),
	(PhysicalCode::Quote, '\'', '"'),
	(PhysicalCode::Backquote, '`', '~'),
	(PhysicalCode::Comma, ',', '<'),
	(PhysicalCode::Period, '.', '>'),
	(PhysicalCode::Slash, '/', '?'),
];

/// Keys of the numeric pad that stand for a character whatever Shift does.
const NUMPAD_KEYS: [(PhysicalCode, char); 15] = [
	(PhysicalCode::Numpad0, '0'),
	(PhysicalCode::Numpad1, '1'),
	(PhysicalCode::Numpad2, '2'),
	(PhysicalCode::Numpad3, '3'),
	(PhysicalCode::Numpad4, '4'),
	(PhysicalCode::Numpad5, '5'),
	(PhysicalCode::Numpad6, '6'),
	(PhysicalCode::Numpad7, '7'),
	(PhysicalCode::Numpad8, '8'),
	(PhysicalCode::Numpad9, '9'),
	(PhysicalCode::NumpadAdd, '+'),
	(PhysicalCode::NumpadSubtract, '-'),
	(PhysicalCode::NumpadMultiply, '*'),
	(PhysicalCode::NumpadDivide, '/'),
	(PhysicalCode::NumpadDecimal, '.'),
];

/// How a character key is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reading {
	/// By its place on the keyboard, as on a US layout: hotkeys and notes.
	Command,
	/// The character the current layout produces: text typed into a dialog.
	Text,
}

/// The parts of a window key event the translation needs. (A `winit` key event cannot be built in
/// tests, its parts can.)
#[derive(Debug, Clone, Copy)]
pub(super) struct WindowKey<'a> {
	pub physical: PhysicalKey,
	pub logical: &'a Key,
	pub state: ElementState,
	pub is_repeat: bool,
	pub held: ModifiersState,
}

impl<'a> WindowKey<'a> {
	pub(super) fn of(event: &'a winit::event::KeyEvent, held: ModifiersState) -> Self {
		Self {
			physical: event.physical_key,
			logical: &event.logical_key,
			state: event.state,
			is_repeat: event.repeat,
			held,
		}
	}
}

/// Translates a window key event into the UI's own type; `None` for keys the UI does not distinguish.
pub(super) fn key_from_window(key: &WindowKey<'_>, reading: Reading) -> Option<KeyEvent> {
	let held = key.held;
	let mut modifiers = KeyModifiers::NONE;
	for (is_held, modifier) in [
		(held.shift_key(), KeyModifiers::SHIFT),
		(held.control_key(), KeyModifiers::CONTROL),
		(held.alt_key(), KeyModifiers::ALT),
		(held.super_key(), KeyModifiers::SUPER),
	] {
		if is_held {
			modifiers |= modifier;
		}
	}
	let code = match named_code(key.logical, held.shift_key()) {
		Some(code) => code,
		None => {
			let by_place = match (reading, key.physical) {
				(Reading::Command, PhysicalKey::Code(code)) => us_character(code, held.shift_key()),
				_ => None,
			};
			// A key with no place on a US keyboard still has the character its layout gives it.
			KeyCode::Char(by_place.or_else(|| layout_character(key.logical))?)
		}
	};
	let kind = match (key.state, key.is_repeat) {
		(ElementState::Released, _) => KeyEventKind::Release,
		(ElementState::Pressed, true) => KeyEventKind::Repeat,
		(ElementState::Pressed, false) => KeyEventKind::Press,
	};
	Some(KeyEvent { code, modifiers, kind })
}

/// Which way to read the key for a modal dialog: plain and shifted character keys are text there,
/// everything else — chords with Ctrl or Alt, named keys — stays a command.
pub(super) fn reading_in_a_dialog(command: &KeyEvent) -> Reading {
	let is_chord = command
		.modifiers
		.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
	if matches!(command.code, KeyCode::Char(character) if character != ' ') && !is_chord {
		Reading::Text
	} else {
		Reading::Command
	}
}

fn named_code(logical: &Key, is_shift_held: bool) -> Option<KeyCode> {
	let Key::Named(named) = logical else {
		return None;
	};
	Some(match named {
		NamedKey::ArrowUp => KeyCode::Up,
		NamedKey::ArrowDown => KeyCode::Down,
		NamedKey::ArrowLeft => KeyCode::Left,
		NamedKey::ArrowRight => KeyCode::Right,
		NamedKey::Home => KeyCode::Home,
		NamedKey::End => KeyCode::End,
		NamedKey::PageUp => KeyCode::PageUp,
		NamedKey::PageDown => KeyCode::PageDown,
		NamedKey::Enter => KeyCode::Enter,
		NamedKey::Escape => KeyCode::Esc,
		NamedKey::Tab if is_shift_held => KeyCode::BackTab,
		NamedKey::Tab => KeyCode::Tab,
		NamedKey::Backspace => KeyCode::Backspace,
		NamedKey::Delete => KeyCode::Delete,
		NamedKey::Insert => KeyCode::Insert,
		NamedKey::Space => KeyCode::Char(' '),
		other => {
			let index = FUNCTION_KEYS.iter().position(|function_key| function_key == other)?;
			KeyCode::F(u8::try_from(index + 1).ok()?)
		}
	})
}

/// The character a US keyboard has on this physical key.
fn us_character(code: PhysicalCode, is_shift_held: bool) -> Option<char> {
	if let Some((_, plain, shifted)) = US_KEYS.iter().find(|(key, _, _)| *key == code) {
		return Some(if is_shift_held { *shifted } else { *plain });
	}
	NUMPAD_KEYS.iter().find(|(key, _)| *key == code).map(|(_, character)| *character)
}

/// The single character the current layout produced; a key that gave several, or none, is not one.
fn layout_character(logical: &Key) -> Option<char> {
	let Key::Character(text) = logical else {
		return None;
	};
	let mut characters = text.chars();
	let character = characters.next()?;
	characters.next().is_none().then_some(character)
}

/// Whether a key pressed in a text field quits the program. A quit chord made of a character key means
/// something else there (Ctrl+C copies), so only the other quit keys (F10) do.
pub(super) fn quits_from_a_field(bindings: &Bindings, key: &KeyEvent) -> bool {
	key.kind == KeyEventKind::Press && !matches!(key.code, KeyCode::Char(_)) && bindings.action_for(key) == Some(Action::Quit)
}

#[cfg(test)]
mod tests;
