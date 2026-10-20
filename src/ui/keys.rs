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
	Panic,
	OctaveUp,
	OctaveDown,
	VelocityUp,
	VelocityDown,
	/// Esc on the main screen: note off for everything the text keyboard holds.
	NotesOff,
	TransposeUp,
	TransposeDown,
	MonitorClear,
	MonitorPause,
	MasterUp,
	MasterDown,
	MasterUpFast,
	MasterDownFast,
	MasterMute,
	/// 0-based channel; shown as one grouped help row.
	SelectChannel(u8),
	ChannelUp,
	ChannelDown,
	ChannelMute,
	ChannelSolo,
	ScrollUp,
	ScrollDown,
	PageUp,
	PageDown,
	ScrollHome,
	ScrollEnd,
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
	bind(Action::Panic, KeyCode::F(9), KeyModifiers::NONE),
	bind(Action::Panic, KeyCode::Char(' '), KeyModifiers::NONE),
	bind(Action::Quit, KeyCode::F(10), KeyModifiers::NONE),
	bind(Action::Quit, KeyCode::Char('c'), KeyModifiers::CONTROL),
	bind(Action::FocusNext, KeyCode::Tab, KeyModifiers::NONE),
	bind(Action::FocusPrevious, KeyCode::BackTab, KeyModifiers::SHIFT),
	bind(Action::FocusPrevious, KeyCode::BackTab, KeyModifiers::NONE),
	bind(Action::ScrollUp, KeyCode::Up, KeyModifiers::NONE),
	bind(Action::ScrollDown, KeyCode::Down, KeyModifiers::NONE),
	bind(Action::PageUp, KeyCode::PageUp, KeyModifiers::NONE),
	bind(Action::PageDown, KeyCode::PageDown, KeyModifiers::NONE),
	bind(Action::ScrollHome, KeyCode::Home, KeyModifiers::NONE),
	bind(Action::ScrollEnd, KeyCode::End, KeyModifiers::NONE),
	bind(Action::OctaveDown, KeyCode::Down, KeyModifiers::CONTROL),
	bind(Action::OctaveUp, KeyCode::Up, KeyModifiers::CONTROL),
	bind(Action::VelocityDown, KeyCode::Left, KeyModifiers::CONTROL),
	bind(Action::VelocityUp, KeyCode::Right, KeyModifiers::CONTROL),
	bind(Action::NotesOff, KeyCode::Esc, KeyModifiers::NONE),
	bind(Action::TransposeDown, KeyCode::Char('['), KeyModifiers::NONE),
	bind(Action::TransposeUp, KeyCode::Char(']'), KeyModifiers::NONE),
	bind(Action::MonitorClear, KeyCode::Char('l'), KeyModifiers::CONTROL),
	bind(Action::MonitorPause, KeyCode::Char('f'), KeyModifiers::CONTROL),
	bind(Action::MasterUp, KeyCode::Char('+'), KeyModifiers::NONE),
	bind(Action::MasterUp, KeyCode::Char('='), KeyModifiers::NONE),
	bind(Action::MasterDown, KeyCode::Char('-'), KeyModifiers::NONE),
	bind(Action::MasterUpFast, KeyCode::Char('+'), KeyModifiers::SHIFT),
	bind(Action::MasterDownFast, KeyCode::Char('_'), KeyModifiers::SHIFT),
	bind(Action::MasterDownFast, KeyCode::Char('_'), KeyModifiers::NONE),
	bind(Action::MasterMute, KeyCode::Char('u'), KeyModifiers::CONTROL),
	bind(Action::SelectChannel(0), KeyCode::Char('1'), KeyModifiers::NONE),
	bind(Action::SelectChannel(1), KeyCode::Char('2'), KeyModifiers::NONE),
	bind(Action::SelectChannel(2), KeyCode::Char('3'), KeyModifiers::NONE),
	bind(Action::SelectChannel(3), KeyCode::Char('4'), KeyModifiers::NONE),
	bind(Action::SelectChannel(4), KeyCode::Char('5'), KeyModifiers::NONE),
	bind(Action::SelectChannel(5), KeyCode::Char('6'), KeyModifiers::NONE),
	bind(Action::SelectChannel(6), KeyCode::Char('7'), KeyModifiers::NONE),
	bind(Action::SelectChannel(7), KeyCode::Char('8'), KeyModifiers::NONE),
	bind(Action::SelectChannel(8), KeyCode::Char('9'), KeyModifiers::NONE),
	bind(Action::SelectChannel(9), KeyCode::Char('0'), KeyModifiers::NONE),
	bind(Action::SelectChannel(10), KeyCode::Char('1'), KeyModifiers::ALT),
	bind(Action::SelectChannel(11), KeyCode::Char('2'), KeyModifiers::ALT),
	bind(Action::SelectChannel(12), KeyCode::Char('3'), KeyModifiers::ALT),
	bind(Action::SelectChannel(13), KeyCode::Char('4'), KeyModifiers::ALT),
	bind(Action::SelectChannel(14), KeyCode::Char('5'), KeyModifiers::ALT),
	bind(Action::SelectChannel(15), KeyCode::Char('6'), KeyModifiers::ALT),
	bind(Action::ChannelDown, KeyCode::Left, KeyModifiers::NONE),
	bind(Action::ChannelUp, KeyCode::Right, KeyModifiers::NONE),
	bind(Action::ChannelMute, KeyCode::Char('x'), KeyModifiers::CONTROL),
	bind(Action::ChannelSolo, KeyCode::Char('o'), KeyModifiers::CONTROL),
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
			Self::Panic => "help.action.panic",
			Self::OctaveUp => "help.action.octave_up",
			Self::OctaveDown => "help.action.octave_down",
			Self::VelocityUp => "help.action.velocity_up",
			Self::VelocityDown => "help.action.velocity_down",
			Self::NotesOff => "help.action.notes_off",
			Self::TransposeUp => "help.action.transpose_up",
			Self::TransposeDown => "help.action.transpose_down",
			Self::MonitorClear => "help.action.monitor_clear",
			Self::MonitorPause => "help.action.monitor_pause",
			Self::MasterUp => "help.action.master_up",
			Self::MasterDown => "help.action.master_down",
			Self::MasterUpFast => "help.action.master_up_fast",
			Self::MasterDownFast => "help.action.master_down_fast",
			Self::MasterMute => "help.action.master_mute",
			Self::SelectChannel(_) => "help.action.select_channel",
			Self::ChannelUp => "help.action.channel_up",
			Self::ChannelDown => "help.action.channel_down",
			Self::ChannelMute => "help.action.channel_mute",
			Self::ChannelSolo => "help.action.channel_solo",
			Self::ScrollUp => "help.action.scroll_up",
			Self::ScrollDown => "help.action.scroll_down",
			Self::PageUp => "help.action.page_up",
			Self::PageDown => "help.action.page_down",
			Self::ScrollHome => "help.action.scroll_home",
			Self::ScrollEnd => "help.action.scroll_end",
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
		KeyCode::Char(' ') => "Space".to_owned(),
		KeyCode::Char(c) => c.to_uppercase().to_string(),
		KeyCode::Tab => "Tab".to_owned(),
		KeyCode::BackTab => "Shift+Tab".to_owned(),
		KeyCode::Esc => "Esc".to_owned(),
		KeyCode::Enter => "Enter".to_owned(),
		KeyCode::PageUp => "PgUp".to_owned(),
		KeyCode::PageDown => "PgDn".to_owned(),
		KeyCode::Home => "Home".to_owned(),
		KeyCode::End => "End".to_owned(),
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
	let mut rows: Vec<(&'static str, Vec<String>)> = Vec::new();
	for binding in BINDINGS {
		let key = binding.action.description_key();
		let label = match binding.action {
			Action::SelectChannel(_) => t!("help.chord.select_channel", locale = locale).into_owned(),
			_ => key_label(binding),
		};
		match rows.iter_mut().find(|(existing, _)| *existing == key) {
			Some((_, labels)) if !labels.contains(&label) => labels.push(label),
			Some(_) => {}
			None => rows.push((key, vec![label])),
		}
	}
	let mut rows: Vec<(String, String)> = rows
		.into_iter()
		.map(|(key, labels)| (labels.join(" / "), t!(key, locale = locale).into_owned()))
		.collect();
	// The piano rows are not commands, but they belong in the same table (F-16).
	rows.push((
		t!("help.chord.white_keys", locale = locale).into_owned(),
		t!("help.action.white_keys", locale = locale).into_owned(),
	));
	rows.push((
		t!("help.chord.black_keys", locale = locale).into_owned(),
		t!("help.action.black_keys", locale = locale).into_owned(),
	));
	rows
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
		assert_eq!(action_for(&press(KeyCode::F(9), KeyModifiers::NONE)), Some(Action::Panic));
		assert_eq!(action_for(&press(KeyCode::Up, KeyModifiers::CONTROL)), Some(Action::OctaveUp));
		assert_eq!(action_for(&press(KeyCode::Left, KeyModifiers::CONTROL)), Some(Action::VelocityDown));
		assert_eq!(action_for(&press(KeyCode::Esc, KeyModifiers::NONE)), Some(Action::NotesOff));
		assert_eq!(
			action_for(&press(KeyCode::Char(']'), KeyModifiers::NONE)),
			Some(Action::TransposeUp)
		);
		assert_eq!(
			action_for(&press(KeyCode::Char('['), KeyModifiers::NONE)),
			Some(Action::TransposeDown)
		);
		assert_eq!(action_for(&press(KeyCode::Char(' '), KeyModifiers::NONE)), Some(Action::Panic));
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
			if let KeyCode::Char(c) = binding.code
				&& c.is_alphabetic()
			{
				assert!(
					binding.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT),
					"{binding:?}"
				);
			}
		}
	}

	#[test]
	fn mixer_keys_map_to_actions() {
		assert_eq!(action_for(&press(KeyCode::Char('+'), KeyModifiers::NONE)), Some(Action::MasterUp));
		assert_eq!(action_for(&press(KeyCode::Char('='), KeyModifiers::NONE)), Some(Action::MasterUp));
		assert_eq!(
			action_for(&press(KeyCode::Char('+'), KeyModifiers::SHIFT)),
			Some(Action::MasterUpFast)
		);
		assert_eq!(action_for(&press(KeyCode::Char('-'), KeyModifiers::NONE)), Some(Action::MasterDown));
		assert_eq!(
			action_for(&press(KeyCode::Char('_'), KeyModifiers::SHIFT)),
			Some(Action::MasterDownFast)
		);
		assert_eq!(
			action_for(&press(KeyCode::Char('u'), KeyModifiers::CONTROL)),
			Some(Action::MasterMute)
		);
		assert_eq!(
			action_for(&press(KeyCode::Char('0'), KeyModifiers::NONE)),
			Some(Action::SelectChannel(9))
		);
		assert_eq!(
			action_for(&press(KeyCode::Char('6'), KeyModifiers::ALT)),
			Some(Action::SelectChannel(15))
		);
		assert_eq!(action_for(&press(KeyCode::Right, KeyModifiers::NONE)), Some(Action::ChannelUp));
		assert_eq!(
			action_for(&press(KeyCode::Char('x'), KeyModifiers::CONTROL)),
			Some(Action::ChannelMute)
		);
		assert_eq!(
			action_for(&press(KeyCode::Char('o'), KeyModifiers::CONTROL)),
			Some(Action::ChannelSolo)
		);
	}

	#[test]
	fn help_lists_every_bound_action_with_its_chords() {
		let rows = help_rows("en");
		for binding in BINDINGS {
			if matches!(binding.action, Action::SelectChannel(_)) {
				continue;
			}
			let label = key_label(binding);
			assert!(
				rows.iter().any(|(keys, _)| keys.split(" / ").any(|chord| chord == label)),
				"{label} missing from help"
			);
		}
		assert!(
			rows.iter()
				.any(|(keys, description)| keys.contains("Alt+1") && description.contains("channel")),
			"grouped channel row"
		);
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
