//! The "input dialog" of the window (spec v1.2 §4.2): a path field, under it a list — suggestions for
//! what is typed, or the history while the field is empty — and the `[OK]` / `[Cancel]` buttons.
//!
//! Built from gterm-widgets controls, so editing, selection, the clipboard, the Tab focus ring and the
//! mouse come from the library. `Tab` moves the focus; a path is completed by picking a suggestion
//! (↑ ↓, a click) — there is no Tab completion (plan v1.2, decisions П-3 and П-4).
//! The directory listing is injected, so the whole dialog is testable without a file system.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use gterm_widgets::widgets::{Block, Button, ButtonState, List, ListState, Paragraph, TextInput, TextInputState};
use gterm_widgets::{Clipboard, Constraint, Layout, Line, Modal, Routed, Style, Ui};
use rust_i18n::t;
use winit::keyboard::{Key, NamedKey};

use super::paint::{BACKGROUND, ERROR, HINT, SELECTED, TEXT};
use super::path_input::{DirEntry, InputKind, split_dir_and_prefix};

/// Size of the dialog in cells; it shrinks to the grid when the window is smaller.
pub const WIDTH: u16 = 88;
pub const HEIGHT: u16 = 16;
/// A directory can hold thousands of entries; the list shows the first ones that match.
const MAX_SUGGESTIONS: usize = 200;

/// How the dialog words itself and what it offers, by what is asked.
impl InputKind {
	fn title_key(self) -> &'static str {
		match self {
			Self::Sf2File => "dialog.sf2_title",
			Self::RecordDir => "dialog.record_title",
			Self::MidiFile => "dialog.midi_file_title",
		}
	}

	fn label_key(self) -> &'static str {
		match self {
			Self::Sf2File | Self::MidiFile => "dialog.sf2_path",
			Self::RecordDir => "dialog.record_dir",
		}
	}

	fn hint_key(self) -> &'static str {
		match self {
			Self::Sf2File | Self::MidiFile => "dialog.input_hint",
			Self::RecordDir => "dialog.folder_hint",
		}
	}

	/// What an empty field means, shown in it as a placeholder; `None` when an empty field means nothing.
	fn empty_meaning_key(self) -> Option<&'static str> {
		match self {
			Self::Sf2File | Self::MidiFile => None,
			Self::RecordDir => Some("dialog.record_dir_default"),
		}
	}

	/// Extensions of the files offered as suggestions; directories are always offered.
	fn extensions(self) -> &'static [&'static str] {
		match self {
			Self::Sf2File => &["sf2"],
			Self::RecordDir => &[],
			Self::MidiFile => &["mid", "midi"],
		}
	}

	/// Whether the answer is a directory: then a path that ends with a separator is an answer, not a
	/// request to look inside.
	fn wants_directory(self) -> bool {
		self == Self::RecordDir
	}
}

/// What the controls of the dialog report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMsg {
	/// The text of the field changed by typing, deleting or pasting.
	Edited(String),
	/// Enter in the field.
	Entered(String),
	/// A row of the list was highlighted with the arrows or clicked.
	Highlighted(usize),
	Accept,
	Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputOutcome {
	Pending,
	Submit(PathBuf),
	Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDialog {
	kind: InputKind,
	field: TextInputState,
	list: ListState,
	/// Rows under the field: suggestions for the typed text, or the history while the field is empty.
	rows: Vec<String>,
	history: Vec<String>,
	ok: ButtonState,
	cancel: ButtonState,
	error: Option<String>,
}

impl InputDialog {
	/// `history` — paths entered before, most recent first.
	pub fn new(kind: InputKind, initial: &str, history: &[PathBuf], list_dir: impl Fn(&Path) -> Vec<DirEntry>) -> Self {
		let mut dialog = Self {
			kind,
			field: TextInputState::with_value(initial),
			list: ListState::default(),
			rows: Vec::new(),
			history: history.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
			ok: ButtonState::new(),
			cancel: ButtonState::new(),
			error: None,
		};
		dialog.refresh_rows(initial, &list_dir);
		dialog
	}

	pub fn kind(&self) -> InputKind {
		self.kind
	}

	pub fn text(&self) -> &str {
		self.field.value()
	}

	pub fn rows(&self) -> &[String] {
		&self.rows
	}

	pub fn highlighted(&self) -> Option<usize> {
		self.list.selected()
	}

	pub fn error(&self) -> Option<&str> {
		self.error.as_deref()
	}

	/// Shows why the submitted path was not accepted; the dialog stays open.
	pub fn set_error(&mut self, message: String) {
		self.error = Some(message);
	}

	/// The dialog as a modal layer; built anew for every event and every frame.
	pub fn ui<'a>(&'a mut self, locale: &str, clipboard: &'a mut dyn Clipboard) -> Ui<'a, InputMsg> {
		let Self {
			kind,
			field,
			list,
			rows,
			ok,
			cancel,
			error,
			..
		} = self;
		let plain = Style::new().fg(TEXT).bg(BACKGROUND);
		let focused = Style::new().underlined();

		let label = t!(kind.label_key(), locale = locale).into_owned();
		let label_width = u16::try_from(label.chars().count() + 1).unwrap_or(u16::MAX);
		let mut input = TextInput::new().style(plain).selection_style(SELECTED);
		if let Some(key) = kind.empty_meaning_key() {
			input = input
				.placeholder(t!(key, locale = locale).into_owned())
				.placeholder_style(Style::new().fg(HINT));
		}
		let field_row = Ui::split(
			Layout::horizontal([Constraint::Length(label_width), Constraint::Fill(1)]),
			vec![
				Ui::widget(Paragraph::new(Line::styled(label, plain.bold()))),
				Ui::stateful(input, field).on_change(InputMsg::Edited).on_submit(InputMsg::Entered),
			],
		);
		let error_row = Ui::widget(Paragraph::new(Line::styled(
			error.clone().unwrap_or_default(),
			Style::new().fg(ERROR),
		)));
		let list_rows = Ui::stateful(
			List::new(rows.iter().map(String::as_str)).style(plain).highlight_style(SELECTED),
			list,
		)
		.on_select(InputMsg::Highlighted);

		let ok_label = format!("[{}]", t!("dialog.ok", locale = locale));
		let cancel_label = format!("[{}]", t!("main.cancel", locale = locale));
		let width_of = |text: &str| u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
		let button = |text: String| Button::new(text).style(plain).focus_style(focused).pressed_style(SELECTED);
		let button_row = Ui::split(
			Layout::horizontal([
				Constraint::Fill(1),
				Constraint::Length(width_of(&ok_label)),
				Constraint::Length(1),
				Constraint::Length(width_of(&cancel_label)),
			]),
			vec![
				Ui::widget(Paragraph::new(Line::styled(
					t!(kind.hint_key(), locale = locale).into_owned(),
					Style::new().fg(HINT),
				))),
				Ui::stateful(button(ok_label), ok).on_activate(InputMsg::Accept),
				Ui::widget(Paragraph::new("")),
				Ui::stateful(button(cancel_label), cancel).on_activate(InputMsg::Cancel),
			],
		);

		let title = format!(" {} ", t!(kind.title_key(), locale = locale));
		let content = Ui::group(
			Block::bordered().title(title).border_style(Style::new().fg(TEXT)),
			Layout::vertical([
				Constraint::Length(1),
				Constraint::Length(1),
				Constraint::Fill(1),
				Constraint::Length(1),
			]),
			vec![field_row, error_row, list_rows, button_row],
		);
		let layer = Modal::centered(WIDTH, HEIGHT)
			.style(plain)
			.content(content)
			.on_close(InputMsg::Cancel);
		Ui::split(Layout::vertical([Constraint::Fill(1)]), Vec::new())
			.modal(layer)
			.clipboard(clipboard)
	}

	/// Applies what the controls reported for one input event. `key` is the pressed key when the event
	/// was a key press: keys no control took still mean something here — ↑ ↓ walk the list from the
	/// field, Enter accepts from the list.
	pub fn settle(&mut self, routed: Routed<InputMsg>, key: Option<&Key>, list_dir: impl Fn(&Path) -> Vec<DirEntry>) -> InputOutcome {
		let was_handled = routed.handled.is_yes();
		let mut outcome = InputOutcome::Pending;
		for message in routed {
			let step = self.update(message, &list_dir);
			if step != InputOutcome::Pending {
				outcome = step;
			}
		}
		if was_handled || outcome != InputOutcome::Pending {
			return outcome;
		}
		match key {
			Some(Key::Named(NamedKey::ArrowDown)) => self.step(1),
			Some(Key::Named(NamedKey::ArrowUp)) => self.step(-1),
			Some(Key::Named(NamedKey::Enter)) => return self.accept(&list_dir),
			_ => {}
		}
		InputOutcome::Pending
	}

	fn update(&mut self, message: InputMsg, list_dir: &impl Fn(&Path) -> Vec<DirEntry>) -> InputOutcome {
		match message {
			InputMsg::Edited(text) => {
				self.error = None;
				self.refresh_rows(&text, list_dir);
			}
			InputMsg::Highlighted(index) => {
				// The list control learns how many rows it has when it is drawn. A key that arrives before
				// the next frame is handled with the old count and may report a row past the end.
				if let Some(last) = self.rows.len().checked_sub(1) {
					let index = index.min(last);
					self.list.select(Some(index));
					self.take_row(index);
				}
			}
			InputMsg::Entered(_) | InputMsg::Accept => return self.accept(list_dir),
			InputMsg::Cancel => return InputOutcome::Cancelled,
		}
		InputOutcome::Pending
	}

	/// Enter / `[OK]`. Asked for a file: a directory is opened in the list, anything else is the answer,
	/// an empty field is none. Asked for a directory: whatever the field holds is the answer, and an
	/// empty field is the answer "the default one" — an empty path.
	fn accept(&mut self, list_dir: &impl Fn(&Path) -> Vec<DirEntry>) -> InputOutcome {
		let text = self.field.value().trim().to_owned();
		if self.kind.wants_directory() {
			return InputOutcome::Submit(PathBuf::from(text));
		}
		if text.is_empty() {
			return InputOutcome::Pending;
		}
		if text.ends_with(['/', '\\']) {
			self.error = None;
			self.refresh_rows(&text, list_dir);
			return InputOutcome::Pending;
		}
		InputOutcome::Submit(PathBuf::from(text))
	}

	/// ↑ / ↓ from the field: highlight the next row and put it into the field.
	fn step(&mut self, delta: isize) {
		if self.rows.is_empty() {
			return;
		}
		let last = self.rows.len() - 1;
		let next = match (self.list.selected(), delta) {
			(None, delta) if delta < 0 => last,
			(None, _) => 0,
			(Some(index), delta) if delta < 0 => index.saturating_sub(1),
			(Some(index), _) => (index + 1).min(last),
		};
		self.list.select(Some(next));
		self.take_row(next);
	}

	/// Puts a row into the field. The rows stay as they are, so the arrows keep walking the same list.
	fn take_row(&mut self, index: usize) {
		if let Some(row) = self.rows.get(index) {
			self.field.set_value(row.clone());
			self.error = None;
		}
	}

	fn refresh_rows(&mut self, text: &str, list_dir: &impl Fn(&Path) -> Vec<DirEntry>) {
		self.rows = if text.is_empty() {
			self.history.clone()
		} else {
			suggestions(text, self.kind, list_dir)
		};
		self.list.select(None);
		self.list.scroll_to(0);
	}
}

/// Entries of the directory part of `text` whose names start with its last component: directories
/// (shown with a trailing separator) and files of the wanted kind. Matching ignores case.
fn suggestions(text: &str, kind: InputKind, list_dir: &impl Fn(&Path) -> Vec<DirEntry>) -> Vec<String> {
	let (dir, prefix) = split_dir_and_prefix(text);
	let prefix = prefix.to_lowercase();
	list_dir(Path::new(if dir.is_empty() { "." } else { &dir }))
		.into_iter()
		.filter(|entry| entry.name.to_lowercase().starts_with(&prefix))
		.filter(|entry| entry.is_dir || has_extension(&entry.name, kind.extensions()))
		.take(MAX_SUGGESTIONS)
		.map(|entry| {
			let mut path = format!("{dir}{}", entry.name);
			if entry.is_dir {
				path.push(MAIN_SEPARATOR);
			}
			path
		})
		.collect()
}

fn has_extension(name: &str, wanted: &[&str]) -> bool {
	Path::new(name)
		.extension()
		.and_then(|extension| extension.to_str())
		.is_some_and(|extension| wanted.iter().any(|wanted| extension.eq_ignore_ascii_case(wanted)))
}

#[cfg(test)]
mod tests;
