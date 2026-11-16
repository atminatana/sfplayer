//! The preset window (spec v1.3, F-26.2): a filter field, the list of the presets of the loaded bank
//! and the `[OK]` / `[Cancel]` buttons.
//!
//! Built from gterm-widgets controls: editing of the field, the Tab ring, the mouse, the wheel and the
//! scrollbar of the list come from the library. Typed text narrows the list; ↑ ↓ from the field walk
//! it; Enter, `[OK]` and a double click on a row choose the highlighted preset.

use gterm_widgets::widgets::{
	Block, Button, ButtonState, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation, TextInput, TextInputState,
};
use gterm_widgets::{Clipboard, Constraint, Layout, Line, Modal, Routed, Style, Ui};
use rust_i18n::t;
use winit::keyboard::{Key, NamedKey};

use super::dialog::{PresetRequest, preset_label};
use super::paint::{BACKGROUND, HINT, LABEL, SELECTED, TEXT};

/// Size of the window in cells; it shrinks to the grid when the window is smaller.
pub const WIDTH: u16 = 88;
pub const HEIGHT: u16 = 18;
/// Rows of the list: what the frame, the filter, the line under it and the buttons leave.
const LIST_ROWS: u16 = HEIGHT - 5;
/// Rows the highlight moves by PgUp / PgDn pressed in the filter field.
const PAGE: isize = 10;
/// Marks the preset that is in use.
const CURRENT_MARK: &str = "●";

/// One preset as the list words it, and what choosing it selects.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
	label: String,
	bank: u16,
	program: u8,
}

/// What the controls of the window report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetMsg {
	/// The text of the filter changed by typing, deleting or pasting.
	Filtered(String),
	/// Enter in the filter field.
	Entered(String),
	/// A row of the list was highlighted with the arrows or clicked.
	Highlighted(usize),
	/// A double click on a row of the list.
	Opened,
	Accept,
	Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetOutcome {
	Pending,
	Chosen { bank: u16, program: u8 },
	Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetDialog {
	rows: Vec<Row>,
	/// The preset in use: its row is marked.
	current: Option<(u16, u8)>,
	filter: TextInputState,
	list: ListState,
	/// The rows that match the filter, as indices into `rows`: what the list shows.
	visible: Vec<usize>,
	ok: ButtonState,
	cancel: ButtonState,
}

impl PresetDialog {
	/// The list starts on the preset in use, or on the first one when the bank plays its default.
	pub fn new(request: &PresetRequest) -> Self {
		let rows: Vec<Row> = request
			.presets
			.iter()
			.map(|preset| Row {
				label: preset_label(preset),
				bank: preset.bank,
				program: preset.program,
			})
			.collect();
		let in_use = request
			.current
			.and_then(|current| rows.iter().position(|row| (row.bank, row.program) == current));
		let start = in_use.or_else(|| (!rows.is_empty()).then_some(0));
		Self {
			visible: (0..rows.len()).collect(),
			rows,
			current: request.current,
			filter: TextInputState::new(),
			list: ListState::with_selected(start),
			ok: ButtonState::new(),
			cancel: ButtonState::new(),
		}
	}

	pub fn filter(&self) -> &str {
		self.filter.value()
	}

	/// The presets the list shows now, as it words them.
	pub fn shown(&self) -> Vec<&str> {
		self.visible.iter().map(|index| self.rows[*index].label.as_str()).collect()
	}

	/// The preset under the highlight, if the list shows any.
	pub fn highlighted(&self) -> Option<(u16, u8)> {
		let row = &self.rows[*self.visible.get(self.list.selected()?)?];
		Some((row.bank, row.program))
	}

	/// The window as a modal layer; built anew for every event and every frame.
	pub fn ui<'a>(&'a mut self, locale: &str, clipboard: &'a mut dyn Clipboard) -> Ui<'a, PresetMsg> {
		let Self {
			rows,
			current,
			filter,
			list,
			visible,
			ok,
			cancel,
		} = self;
		let plain = Style::new().fg(TEXT).bg(BACKGROUND);
		let focused = Style::new().underlined();

		let label = t!("dialog.filter", locale = locale).into_owned();
		let label_width = u16::try_from(label.chars().count() + 1).unwrap_or(u16::MAX);
		let field_row = Ui::split(
			Layout::horizontal([Constraint::Length(label_width), Constraint::Fill(1)]),
			vec![
				Ui::widget(Paragraph::new(Line::styled(label, plain.bold()))),
				Ui::stateful(TextInput::new().style(plain).selection_style(SELECTED), filter)
					.on_change(PresetMsg::Filtered)
					.on_submit(PresetMsg::Entered),
			],
		);

		// With nothing to show the row under the field says why.
		let nothing = match (visible.is_empty(), rows.is_empty()) {
			(false, _) => String::new(),
			(true, true) => t!("dialog.empty", locale = locale).into_owned(),
			(true, false) => t!("dialog.no_match", locale = locale).into_owned(),
		};
		let nothing_row = Ui::widget(Paragraph::new(Line::styled(nothing, Style::new().fg(HINT))));

		let lines: Vec<String> = visible
			.iter()
			.map(|index| {
				let row = &rows[*index];
				let mark = if *current == Some((row.bank, row.program)) {
					CURRENT_MARK
				} else {
					" "
				};
				format!("{mark} {}", row.label)
			})
			.collect();
		let mut presets = List::new(lines).style(plain).highlight_style(SELECTED);
		// A list that fits needs no bar.
		if visible.len() > usize::from(LIST_ROWS) {
			let bar = Scrollbar::new(ScrollbarOrientation::Vertical)
				.style(Style::new().fg(LABEL))
				.thumb_style(Style::new().fg(TEXT));
			presets = presets.scrollbar(bar);
		}
		let list_rows = Ui::stateful(presets, list)
			.on_select(PresetMsg::Highlighted)
			.on_double_click(PresetMsg::Opened);

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
					t!("dialog.preset_hint", locale = locale).into_owned(),
					Style::new().fg(HINT),
				))),
				Ui::stateful(button(ok_label), ok).on_activate(PresetMsg::Accept),
				Ui::widget(Paragraph::new("")),
				Ui::stateful(button(cancel_label), cancel).on_activate(PresetMsg::Cancel),
			],
		);

		let title = format!(" {} ", t!("dialog.preset_title", locale = locale));
		let content = Ui::group(
			Block::bordered().title(title).border_style(Style::new().fg(TEXT)),
			Layout::vertical([
				Constraint::Length(1),
				Constraint::Length(1),
				Constraint::Fill(1),
				Constraint::Length(1),
			]),
			vec![field_row, nothing_row, list_rows, button_row],
		);
		let layer = Modal::centered(WIDTH, HEIGHT)
			.style(plain)
			.content(content)
			.on_close(PresetMsg::Cancel);
		Ui::split(Layout::vertical([Constraint::Fill(1)]), Vec::new())
			.modal(layer)
			.clipboard(clipboard)
	}

	/// Applies what the controls reported for one input event. `key` is the pressed key when the event
	/// was a key press: keys no control took still mean something here — ↑ ↓ PgUp PgDn walk the list
	/// from the field, Enter chooses from the list.
	pub fn settle(&mut self, routed: Routed<PresetMsg>, key: Option<&Key>) -> PresetOutcome {
		let was_handled = routed.handled.is_yes();
		let mut outcome = PresetOutcome::Pending;
		for message in routed {
			let step = self.update(message);
			if step != PresetOutcome::Pending {
				outcome = step;
			}
		}
		if was_handled || outcome != PresetOutcome::Pending {
			return outcome;
		}
		match key {
			Some(Key::Named(NamedKey::ArrowDown)) => self.step(1),
			Some(Key::Named(NamedKey::ArrowUp)) => self.step(-1),
			Some(Key::Named(NamedKey::PageDown)) => self.step(PAGE),
			Some(Key::Named(NamedKey::PageUp)) => self.step(-PAGE),
			Some(Key::Named(NamedKey::Enter)) => return self.accept(),
			_ => {}
		}
		PresetOutcome::Pending
	}

	fn update(&mut self, message: PresetMsg) -> PresetOutcome {
		match message {
			PresetMsg::Filtered(text) => self.narrow(&text),
			PresetMsg::Highlighted(index) => {
				// The list control learns how many rows it has when it is drawn. A key that arrives before
				// the next frame is handled with the old count and may report a row past the end.
				if let Some(last) = self.visible.len().checked_sub(1) {
					self.list.select(Some(index.min(last)));
				}
			}
			PresetMsg::Entered(_) | PresetMsg::Opened | PresetMsg::Accept => return self.accept(),
			PresetMsg::Cancel => return PresetOutcome::Cancelled,
		}
		PresetOutcome::Pending
	}

	/// Keeps the presets whose line has `text` in it, whatever the case; the highlight goes to the
	/// first of them.
	fn narrow(&mut self, text: &str) {
		self.visible = matching(self.rows.iter().map(|row| row.label.as_str()), text);
		self.list.select((!self.visible.is_empty()).then_some(0));
		self.list.scroll_to(0);
	}

	/// Moves the highlight by `delta` rows, stopping at the ends of the list.
	fn step(&mut self, delta: isize) {
		let Some(last) = self.visible.len().checked_sub(1) else {
			return;
		};
		let next = match self.list.selected() {
			None if delta < 0 => last,
			None => 0,
			Some(index) => index.saturating_add_signed(delta).min(last),
		};
		self.list.select(Some(next));
	}

	/// Enter, `[OK]`, a double click: the highlighted preset is the answer; with nothing to choose
	/// from the window stays as it is.
	fn accept(&self) -> PresetOutcome {
		match self.highlighted() {
			Some((bank, program)) => PresetOutcome::Chosen { bank, program },
			None => PresetOutcome::Pending,
		}
	}
}

/// The places of the lines that have `needle` in them, whatever the case; every line for an empty needle.
pub fn matching<'a>(lines: impl Iterator<Item = &'a str>, needle: &str) -> Vec<usize> {
	let needle = needle.to_lowercase();
	lines
		.enumerate()
		.filter(|(_, line)| needle.is_empty() || line.to_lowercase().contains(&needle))
		.map(|(index, _)| index)
		.collect()
}

#[cfg(test)]
mod tests;
