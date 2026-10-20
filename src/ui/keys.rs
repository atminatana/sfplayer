//! Command hotkeys (spec §6, ADR 0004): one table drives key handling, the F1 help and the README.
//! No plain letters here — the two bottom rows belong to the text keyboard (F-16).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
	Help,
	Quit,
	FocusNext,
	FocusPrevious,
	MidiDialog,
	AudioDialog,
	Sf2Dialog,
	PresetDialog,
	RecordFormatDialog,
	Rescan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
	pub action: Action,
	pub code: KeyCode,
	pub modifiers: KeyModifiers,
}

const fn bind(action: Action, code: KeyCode, modifiers: KeyModifiers) -> Binding {
	Binding { action, code, modifiers }
}

/// Default bindings in display order (spec §6 table).
pub const BINDINGS: &[Binding] = &[
	bind(Action::Help, KeyCode::F(1), KeyModifiers::NONE),
	bind(Action::MidiDialog, KeyCode::F(2), KeyModifiers::NONE),
	bind(Action::AudioDialog, KeyCode::F(3), KeyModifiers::NONE),
	bind(Action::Sf2Dialog, KeyCode::F(4), KeyModifiers::NONE),
	bind(Action::PresetDialog, KeyCode::F(5), KeyModifiers::NONE),
	bind(Action::RecordFormatDialog, KeyCode::F(6), KeyModifiers::NONE),
	bind(Action::Rescan, KeyCode::F(8), KeyModifiers::NONE),
	bind(Action::Quit, KeyCode::F(10), KeyModifiers::NONE),
	bind(Action::Quit, KeyCode::Char('c'), KeyModifiers::CONTROL),
	bind(Action::FocusNext, KeyCode::Tab, KeyModifiers::NONE),
	bind(Action::FocusPrevious, KeyCode::BackTab, KeyModifiers::SHIFT),
	bind(Action::FocusPrevious, KeyCode::BackTab, KeyModifiers::NONE),
];

/// Maps a key press to a command; `None` means the key is free for other handlers (dialogs, text keyboard).
pub fn action_for(key: &KeyEvent) -> Option<Action> {
	BINDINGS
		.iter()
		.find(|binding| binding.code == key.code && binding.modifiers == key.modifiers)
		.map(|binding| binding.action)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
		KeyEvent::new(code, modifiers)
	}

	#[test]
	fn function_keys_and_ctrl_c_map_to_actions() {
		assert_eq!(action_for(&press(KeyCode::F(1), KeyModifiers::NONE)), Some(Action::Help));
		assert_eq!(action_for(&press(KeyCode::F(10), KeyModifiers::NONE)), Some(Action::Quit));
		assert_eq!(action_for(&press(KeyCode::F(2), KeyModifiers::NONE)), Some(Action::MidiDialog));
		assert_eq!(action_for(&press(KeyCode::F(3), KeyModifiers::NONE)), Some(Action::AudioDialog));
		assert_eq!(action_for(&press(KeyCode::F(4), KeyModifiers::NONE)), Some(Action::Sf2Dialog));
		assert_eq!(action_for(&press(KeyCode::F(5), KeyModifiers::NONE)), Some(Action::PresetDialog));
		assert_eq!(
			action_for(&press(KeyCode::F(6), KeyModifiers::NONE)),
			Some(Action::RecordFormatDialog)
		);
		assert_eq!(action_for(&press(KeyCode::F(8), KeyModifiers::NONE)), Some(Action::Rescan));
		assert_eq!(action_for(&press(KeyCode::Char('c'), KeyModifiers::CONTROL)), Some(Action::Quit));
		assert_eq!(action_for(&press(KeyCode::Tab, KeyModifiers::NONE)), Some(Action::FocusNext));
		assert_eq!(
			action_for(&press(KeyCode::BackTab, KeyModifiers::SHIFT)),
			Some(Action::FocusPrevious)
		);
	}

	#[test]
	fn plain_letters_are_never_commands() {
		for letter in "zxcvbnm,./sdghjl;".chars() {
			assert_eq!(action_for(&press(KeyCode::Char(letter), KeyModifiers::NONE)), None, "{letter}");
			assert_eq!(action_for(&press(KeyCode::Char(letter), KeyModifiers::SHIFT)), None, "{letter}");
		}
	}

	#[test]
	fn no_binding_uses_a_bare_letter() {
		for binding in BINDINGS {
			if let KeyCode::Char(c) = binding.code {
				assert!(
					c.is_alphabetic() && binding.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT),
					"{binding:?}"
				);
			}
		}
	}
}
