//! Command hotkeys (spec §6, ADR 0004): one table drives key handling, the F1 help and the README.
//! No plain letters here — the two bottom rows belong to the text keyboard (F-16).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rust_i18n::t;

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

impl Action {
	/// Dictionary key of the one-line description shown in the F1 help.
	pub fn description_key(self) -> &'static str {
		match self {
			Self::Help => "help.action.help",
			Self::Quit => "help.action.quit",
			Self::FocusNext => "help.action.focus_next",
			Self::FocusPrevious => "help.action.focus_previous",
			Self::MidiDialog => "help.action.midi_dialog",
			Self::AudioDialog => "help.action.audio_dialog",
			Self::Sf2Dialog => "help.action.sf2_dialog",
			Self::PresetDialog => "help.action.preset_dialog",
			Self::RecordFormatDialog => "help.action.record_format_dialog",
			Self::Rescan => "help.action.rescan",
		}
	}
}

/// Human-readable chord ("Ctrl+C", "Shift+Tab", "F10") for help and README.
pub fn key_label(binding: &Binding) -> String {
	let mut parts: Vec<String> = Vec::new();
	if binding.modifiers.contains(KeyModifiers::CONTROL) {
		parts.push("Ctrl".to_owned());
	}
	if binding.modifiers.contains(KeyModifiers::ALT) {
		parts.push("Alt".to_owned());
	}
	if binding.modifiers.contains(KeyModifiers::SHIFT) && !matches!(binding.code, KeyCode::BackTab) {
		parts.push("Shift".to_owned());
	}
	let key = match binding.code {
		KeyCode::F(n) => format!("F{n}"),
		KeyCode::Char(c) => c.to_uppercase().to_string(),
		KeyCode::Tab => "Tab".to_owned(),
		KeyCode::BackTab => "Shift+Tab".to_owned(),
		KeyCode::Esc => "Esc".to_owned(),
		KeyCode::Enter => "Enter".to_owned(),
		KeyCode::Up => "\u{2191}".to_owned(),
		KeyCode::Down => "\u{2193}".to_owned(),
		KeyCode::Left => "\u{2190}".to_owned(),
		KeyCode::Right => "\u{2192}".to_owned(),
		other => format!("{other:?}"),
	};
	parts.push(key);
	parts.join("+")
}

/// One help row per action, in `BINDINGS` order; several chords for one action are joined with " / ".
pub fn help_rows(locale: &str) -> Vec<(String, String)> {
	let mut rows: Vec<(Action, Vec<String>)> = Vec::new();
	for binding in BINDINGS {
		let label = key_label(binding);
		match rows.iter_mut().find(|(action, _)| *action == binding.action) {
			Some((_, labels)) if !labels.contains(&label) => labels.push(label),
			Some(_) => {}
			None => rows.push((binding.action, vec![label])),
		}
	}
	rows.into_iter()
		.map(|(action, labels)| (labels.join(" / "), t!(action.description_key(), locale = locale).into_owned()))
		.collect()
}

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

	#[test]
	fn help_lists_every_bound_action_with_its_chords() {
		let rows = help_rows("en");
		for binding in BINDINGS {
			let label = key_label(binding);
			assert!(
				rows.iter().any(|(keys, _)| keys.split(" / ").any(|chord| chord == label)),
				"{label} missing from help"
			);
		}
		let quit = rows.iter().find(|(keys, _)| keys.contains("F10")).expect("quit row");
		assert_eq!(quit.0, "F10 / Ctrl+C");
		assert!(
			!quit.1.is_empty() && !quit.1.starts_with("help."),
			"description must be translated: {}",
			quit.1
		);
		let distinct: std::collections::HashSet<&String> = rows.iter().map(|(_, description)| description).collect();
		assert_eq!(distinct.len(), rows.len(), "every action has its own description");
	}

	#[test]
	fn key_labels_are_readable() {
		assert_eq!(key_label(&bind(Action::Quit, KeyCode::Char('c'), KeyModifiers::CONTROL)), "Ctrl+C");
		assert_eq!(
			key_label(&bind(Action::FocusPrevious, KeyCode::BackTab, KeyModifiers::SHIFT)),
			"Shift+Tab"
		);
		assert_eq!(key_label(&bind(Action::Help, KeyCode::F(1), KeyModifiers::NONE)), "F1");
		assert_eq!(key_label(&bind(Action::Help, KeyCode::Up, KeyModifiers::CONTROL)), "Ctrl+\u{2191}");
	}
}
