//! Command hotkeys (spec §6 / v1.1 §3, ADR 0004, 0010): one table drives key handling, the F1 help and the README.
//! No plain letters here — the two bottom rows belong to the text keyboard (F-16).
//! Defaults live in `BINDINGS`; `[keys]` and `[keyboard]` in the config override them (ADR 0007).

use std::collections::BTreeMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rust_i18n::t;

use crate::config::KeyboardConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
	Help,
	Quit,
	MidiDialog,
	AudioDialog,
	Sf2Dialog,
	PresetDialog,
	EffectsDialog,
	RecordDialog,
	Rescan,
	RecordStart,
	RecordStop,
	Panic,
	OctaveUp,
	OctaveDown,
	VelocityUp,
	VelocityDown,
	/// Esc on the main screen: note off for everything the text keyboard holds.
	NotesOff,
	TransposeUp,
	TransposeDown,
	ToggleReverb,
	ToggleChorus,
	MasterUp,
	MasterDown,
	MasterUpFast,
	MasterDownFast,
	MasterMute,
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
	bind(Action::EffectsDialog, KeyCode::F(6), KeyModifiers::NONE),
	bind(Action::RecordDialog, KeyCode::F(7), KeyModifiers::NONE),
	bind(Action::RecordStart, KeyCode::Home, KeyModifiers::NONE),
	bind(Action::RecordStop, KeyCode::End, KeyModifiers::NONE),
	bind(Action::Rescan, KeyCode::F(8), KeyModifiers::NONE),
	bind(Action::Panic, KeyCode::F(9), KeyModifiers::NONE),
	bind(Action::Panic, KeyCode::Char(' '), KeyModifiers::NONE),
	bind(Action::Quit, KeyCode::F(10), KeyModifiers::NONE),
	bind(Action::Quit, KeyCode::Char('c'), KeyModifiers::CONTROL),
	bind(Action::OctaveDown, KeyCode::Down, KeyModifiers::CONTROL),
	bind(Action::OctaveUp, KeyCode::Up, KeyModifiers::CONTROL),
	bind(Action::VelocityDown, KeyCode::Left, KeyModifiers::CONTROL),
	bind(Action::VelocityUp, KeyCode::Right, KeyModifiers::CONTROL),
	bind(Action::NotesOff, KeyCode::Esc, KeyModifiers::NONE),
	bind(Action::TransposeDown, KeyCode::Char('['), KeyModifiers::NONE),
	bind(Action::TransposeUp, KeyCode::Char(']'), KeyModifiers::NONE),
	bind(Action::ToggleReverb, KeyCode::Char('r'), KeyModifiers::ALT),
	bind(Action::ToggleChorus, KeyCode::Char('c'), KeyModifiers::ALT),
	bind(Action::MasterUp, KeyCode::Char('+'), KeyModifiers::NONE),
	bind(Action::MasterUp, KeyCode::Char('='), KeyModifiers::NONE),
	bind(Action::MasterDown, KeyCode::Char('-'), KeyModifiers::NONE),
	bind(Action::MasterUpFast, KeyCode::Char('+'), KeyModifiers::SHIFT),
	bind(Action::MasterDownFast, KeyCode::Char('_'), KeyModifiers::SHIFT),
	bind(Action::MasterDownFast, KeyCode::Char('_'), KeyModifiers::NONE),
	bind(Action::MasterMute, KeyCode::Char('u'), KeyModifiers::CONTROL),
];

/// Command names accepted in `[keys]` (snake_case of the action).
const COMMAND_NAMES: &[(&str, Action)] = &[
	("help", Action::Help),
	("quit", Action::Quit),
	("midi_dialog", Action::MidiDialog),
	("audio_dialog", Action::AudioDialog),
	("sf2_dialog", Action::Sf2Dialog),
	("preset_dialog", Action::PresetDialog),
	("effects_dialog", Action::EffectsDialog),
	("record_dialog", Action::RecordDialog),
	("rescan", Action::Rescan),
	("record_start", Action::RecordStart),
	("record_stop", Action::RecordStop),
	("panic", Action::Panic),
	("octave_up", Action::OctaveUp),
	("octave_down", Action::OctaveDown),
	("velocity_up", Action::VelocityUp),
	("velocity_down", Action::VelocityDown),
	("notes_off", Action::NotesOff),
	("transpose_up", Action::TransposeUp),
	("transpose_down", Action::TransposeDown),
	("toggle_reverb", Action::ToggleReverb),
	("toggle_chorus", Action::ToggleChorus),
	("master_up", Action::MasterUp),
	("master_down", Action::MasterDown),
	("master_up_fast", Action::MasterUpFast),
	("master_down_fast", Action::MasterDownFast),
	("master_mute", Action::MasterMute),
];

/// Parses a `[keys]` command name.
pub fn action_by_name(name: &str) -> Option<Action> {
	COMMAND_NAMES.iter().find(|(known, _)| *known == name).map(|(_, action)| *action)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BindingError {
	#[error("[keys] {key}: unknown command (see the F1 help for names)")]
	UnknownCommand { key: String },
	#[error("[keys] {key}: cannot parse chord {chord:?}")]
	BadChord { key: String, chord: String },
	#[error("[keys] {key}: {chord:?} is a bare letter, which the text keyboard needs for notes")]
	BareLetter { key: String, chord: String },
	#[error("[keys] {key}: {chord} is already used by {other}")]
	Conflict { key: String, chord: String, other: String },
}

/// Parses "Ctrl+Alt+F5", "Shift+Tab", "Space", "]", "Shift++" into a key code and modifiers. Case-insensitive.
pub fn parse_chord(chord: &str) -> Option<(KeyCode, KeyModifiers)> {
	let trimmed = chord.trim();
	// A trailing '+' is the key itself ("+" or "Shift++"), not a separator.
	let (modifier_part, key) = match trimmed.strip_suffix('+') {
		Some(prefix) => (prefix.strip_suffix('+').unwrap_or(prefix), "+"),
		None => trimmed.rsplit_once('+').unwrap_or(("", trimmed)),
	};
	let key = key.trim();
	if key.is_empty() {
		return None;
	}
	let mut modifiers = KeyModifiers::NONE;
	for part in modifier_part.split('+').map(str::trim).filter(|part| !part.is_empty()) {
		match part.to_ascii_lowercase().as_str() {
			"ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
			"alt" => modifiers |= KeyModifiers::ALT,
			"shift" => modifiers |= KeyModifiers::SHIFT,
			_ => return None,
		}
	}
	let lower = key.to_ascii_lowercase();
	let code = match lower.as_str() {
		"space" => KeyCode::Char(' '),
		"tab" => {
			if modifiers.contains(KeyModifiers::SHIFT) {
				KeyCode::BackTab
			} else {
				KeyCode::Tab
			}
		}
		"esc" | "escape" => KeyCode::Esc,
		"enter" | "return" => KeyCode::Enter,
		"up" | "↑" => KeyCode::Up,
		"down" | "↓" => KeyCode::Down,
		"left" | "←" => KeyCode::Left,
		"right" | "→" => KeyCode::Right,
		"home" => KeyCode::Home,
		"end" => KeyCode::End,
		"pgup" | "pageup" => KeyCode::PageUp,
		"pgdn" | "pagedown" => KeyCode::PageDown,
		"backspace" => KeyCode::Backspace,
		"delete" | "del" => KeyCode::Delete,
		"insert" | "ins" => KeyCode::Insert,
		_ => {
			if let Some(number) = lower
				.strip_prefix('f')
				.and_then(|n| n.parse::<u8>().ok())
				.filter(|n| (1..=24).contains(n))
			{
				KeyCode::F(number)
			} else {
				let mut chars = key.chars();
				let c = chars.next()?;
				if chars.next().is_some() {
					return None;
				}
				KeyCode::Char(c.to_ascii_lowercase())
			}
		}
	};
	Some((code, modifiers))
}

/// The active key table: defaults with `[keys]` / `[keyboard]` overrides applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
	bindings: Vec<Binding>,
}

impl Default for Bindings {
	fn default() -> Self {
		Self {
			bindings: BINDINGS.to_vec(),
		}
	}
}

impl Bindings {
	/// Applies overrides: every listed command loses its default chords and gets the configured ones
	/// (several, comma-separated). `[keyboard]` octave / velocity alternatives are added on top of the defaults.
	pub fn from_config(keys: &BTreeMap<String, String>, keyboard: &KeyboardConfig) -> Result<Self, BindingError> {
		let mut bindings = Self::default();
		for (key, chords) in keys {
			let action = action_by_name(key).ok_or_else(|| BindingError::UnknownCommand { key: key.clone() })?;
			bindings.bindings.retain(|binding| binding.action != action);
			for chord in chords.split(',').map(str::trim).filter(|chord| !chord.is_empty()) {
				bindings.add(key, chord, action)?;
			}
		}
		for (key, chord, action) in [
			("keyboard.octave_up", &keyboard.octave_up, Action::OctaveUp),
			("keyboard.octave_down", &keyboard.octave_down, Action::OctaveDown),
			("keyboard.velocity_up", &keyboard.velocity_up, Action::VelocityUp),
			("keyboard.velocity_down", &keyboard.velocity_down, Action::VelocityDown),
		] {
			if let Some(chord) = chord {
				bindings.add(key, chord, action)?;
			}
		}
		Ok(bindings)
	}

	fn add(&mut self, key: &str, chord: &str, action: Action) -> Result<(), BindingError> {
		let (code, modifiers) = parse_chord(chord).ok_or_else(|| BindingError::BadChord {
			key: key.to_owned(),
			chord: chord.to_owned(),
		})?;
		if let KeyCode::Char(c) = code
			&& c.is_alphabetic()
			&& !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
		{
			return Err(BindingError::BareLetter {
				key: key.to_owned(),
				chord: chord.to_owned(),
			});
		}
		if let Some(other) = self
			.bindings
			.iter()
			.find(|binding| binding.code == code && binding.modifiers == modifiers && binding.action != action)
		{
			return Err(BindingError::Conflict {
				key: key.to_owned(),
				chord: key_label_of(code, modifiers),
				other: command_name(other.action),
			});
		}
		if !self
			.bindings
			.iter()
			.any(|binding| binding.code == code && binding.modifiers == modifiers)
		{
			self.bindings.push(bind(action, code, modifiers));
		}
		Ok(())
	}

	/// Maps a key press to a command; `None` means the key is free for other handlers (dialogs, text keyboard).
	pub fn action_for(&self, key: &KeyEvent) -> Option<Action> {
		self.bindings
			.iter()
			.find(|binding| binding.code == key.code && binding.modifiers == key.modifiers)
			.map(|binding| binding.action)
	}

	pub fn iter(&self) -> impl Iterator<Item = &Binding> {
		self.bindings.iter()
	}

	/// One help row per action, in table order; several chords for one action are joined with " / ".
	pub fn help_rows(&self, locale: &str) -> Vec<(String, String)> {
		let mut rows: Vec<(&'static str, Vec<String>)> = Vec::new();
		for binding in &self.bindings {
			let key = binding.action.description_key();
			let label = key_label(binding);
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
}

/// `[keys]` name of an action (for messages and the README).
pub fn command_name(action: Action) -> String {
	COMMAND_NAMES
		.iter()
		.find(|(_, known)| *known == action)
		.map_or_else(|| format!("{action:?}"), |(name, _)| (*name).to_owned())
}

impl Action {
	/// Dictionary key of the one-line description shown in the F1 help.
	pub fn description_key(self) -> &'static str {
		match self {
			Self::Help => "help.action.help",
			Self::Quit => "help.action.quit",
			Self::MidiDialog => "help.action.midi_dialog",
			Self::AudioDialog => "help.action.audio_dialog",
			Self::Sf2Dialog => "help.action.sf2_dialog",
			Self::PresetDialog => "help.action.preset_dialog",
			Self::EffectsDialog => "help.action.effects_dialog",
			Self::RecordDialog => "help.action.record_dialog",
			Self::Rescan => "help.action.rescan",
			Self::RecordStart => "help.action.record_start",
			Self::RecordStop => "help.action.record_stop",
			Self::Panic => "help.action.panic",
			Self::OctaveUp => "help.action.octave_up",
			Self::OctaveDown => "help.action.octave_down",
			Self::VelocityUp => "help.action.velocity_up",
			Self::VelocityDown => "help.action.velocity_down",
			Self::NotesOff => "help.action.notes_off",
			Self::TransposeUp => "help.action.transpose_up",
			Self::TransposeDown => "help.action.transpose_down",
			Self::ToggleReverb => "help.action.toggle_reverb",
			Self::ToggleChorus => "help.action.toggle_chorus",
			Self::MasterUp => "help.action.master_up",
			Self::MasterDown => "help.action.master_down",
			Self::MasterUpFast => "help.action.master_up_fast",
			Self::MasterDownFast => "help.action.master_down_fast",
			Self::MasterMute => "help.action.master_mute",
		}
	}
}

/// Human-readable chord ("Ctrl+C", "Shift+Tab", "F10") for help and README.
pub fn key_label(binding: &Binding) -> String {
	key_label_of(binding.code, binding.modifiers)
}

fn key_label_of(code: KeyCode, modifiers: KeyModifiers) -> String {
	let mut parts: Vec<String> = Vec::new();
	if modifiers.contains(KeyModifiers::CONTROL) {
		parts.push("Ctrl".to_owned());
	}
	if modifiers.contains(KeyModifiers::ALT) {
		parts.push("Alt".to_owned());
	}
	if modifiers.contains(KeyModifiers::SHIFT) && !matches!(code, KeyCode::BackTab) {
		parts.push("Shift".to_owned());
	}
	let key = match code {
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

#[cfg(test)]
mod tests;
