//! Key presses as the UI logic sees them, independent of the input backend (plan v1.2, step 1.1).
//!
//! Bindings, dialogs and the key table work with these types only; the event loop converts whatever
//! the backend delivers. The shape follows the terminal events the UI was first written against, which
//! is why Shift+Tab is its own key.

use std::ops::{BitOr, BitOrAssign};

/// The keys the UI distinguishes. Anything else the backend reports is dropped by the event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
	/// A character key; a space is `Char(' ')`.
	Char(char),
	/// A function key, numbered from 1.
	F(u8),
	Up,
	Down,
	Left,
	Right,
	Home,
	End,
	PageUp,
	PageDown,
	Enter,
	Esc,
	Tab,
	/// Shift+Tab.
	BackTab,
	Backspace,
	Delete,
	Insert,
}

/// A set of held modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
	pub const NONE: Self = Self(0);
	pub const SHIFT: Self = Self(1);
	pub const CONTROL: Self = Self(1 << 1);
	pub const ALT: Self = Self(1 << 2);
	/// Super / Meta / Hyper: no command uses it, but a chord that carries it must not match one that does not.
	pub const SUPER: Self = Self(1 << 3);

	/// Whether every modifier of `other` is held.
	pub const fn contains(self, other: Self) -> bool {
		self.0 & other.0 == other.0
	}

	/// Whether at least one modifier of `other` is held.
	pub const fn intersects(self, other: Self) -> bool {
		self.0 & other.0 != 0
	}

	pub const fn is_empty(self) -> bool {
		self.0 == 0
	}
}

impl BitOr for KeyModifiers {
	type Output = Self;

	fn bitor(self, other: Self) -> Self {
		Self(self.0 | other.0)
	}
}

impl BitOrAssign for KeyModifiers {
	fn bitor_assign(&mut self, other: Self) {
		self.0 |= other.0;
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyEventKind {
	Press,
	/// Auto-repeat of a held key.
	Repeat,
	Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
	pub code: KeyCode,
	pub modifiers: KeyModifiers,
	pub kind: KeyEventKind,
}

impl KeyEvent {
	/// A key press (not a repeat, not a release).
	pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
		Self {
			code,
			modifiers,
			kind: KeyEventKind::Press,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn modifier_sets_combine_and_answer_membership_questions() {
		let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
		assert!(ctrl_alt.contains(KeyModifiers::CONTROL));
		assert!(ctrl_alt.contains(ctrl_alt));
		assert!(!ctrl_alt.contains(KeyModifiers::SHIFT));
		assert!(!KeyModifiers::CONTROL.contains(ctrl_alt), "a subset does not contain the whole set");
		assert!(ctrl_alt.intersects(KeyModifiers::ALT | KeyModifiers::SHIFT));
		assert!(!ctrl_alt.intersects(KeyModifiers::SHIFT | KeyModifiers::SUPER));

		let mut held = KeyModifiers::NONE;
		assert!(held.is_empty());
		assert_eq!(held, KeyModifiers::default());
		held |= KeyModifiers::SHIFT;
		assert_eq!(held, KeyModifiers::SHIFT);
		assert!(!held.is_empty());
	}

	#[test]
	fn a_new_key_event_is_a_press() {
		let event = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
		assert_eq!(event.kind, KeyEventKind::Press);
		assert_eq!(event.code, KeyCode::F(1));
	}
}
