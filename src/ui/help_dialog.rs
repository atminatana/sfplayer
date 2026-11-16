//! The help window (spec v1.3, F-26.4): the table "keys — what they do" and the `[Close]` button. The
//! rows are the bindings in force, so the help can never disagree with the handler (ADR 0004).
//!
//! Built from gterm-widgets controls: the table with its highlight, the scrollbar, the wheel and the
//! Tab ring come from the library. ↑ ↓ PgUp PgDn Home End move the highlight, and the table follows it.

use gterm_widgets::widgets::{Block, Button, ButtonState, Paragraph, Row, Scrollbar, ScrollbarOrientation, Table, TableState};
use gterm_widgets::{Constraint, Layout, Line, Modal, Routed, Style, Text, Ui};
use rust_i18n::t;
use winit::keyboard::{Key, NamedKey};

use super::paint::{BACKGROUND, HINT, LABEL, SELECTED, TEXT};

/// The widest the window gets; it shrinks to the grid when the grid is narrower.
pub const WIDTH: u16 = 88;
/// Rows of the grid left free above and below the window when the whole table does not fit.
pub const MARGIN_ROWS: u16 = 2;
/// Rows of the window that are not the table: the frame and the row of the button.
pub const CHROME_ROWS: u16 = 3;
/// Cells between the keys and what they do.
const COLUMN_GAP: u16 = 2;

/// What the controls of the window report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpMsg {
	Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpOutcome {
	Pending,
	Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpDialog {
	/// The keys of a command and what the command does, one row per command.
	rows: Vec<(String, String)>,
	table: TableState,
	close: ButtonState,
}

impl HelpDialog {
	/// The highlight starts on the first row.
	pub fn new(rows: Vec<(String, String)>) -> Self {
		let first = (!rows.is_empty()).then_some(0);
		Self {
			rows,
			table: TableState::with_selected(first),
			close: ButtonState::new(),
		}
	}

	/// The row under the highlight.
	pub fn highlighted(&self) -> Option<usize> {
		self.table.selected()
	}

	/// Rows the table gets on a grid of `grid_rows` rows: all of them when there is room.
	pub fn table_rows(&self, grid_rows: u16) -> u16 {
		let wanted = u16::try_from(self.rows.len()).unwrap_or(u16::MAX);
		let room = grid_rows.saturating_sub(2 * MARGIN_ROWS + CHROME_ROWS);
		wanted.min(room).max(1)
	}

	/// The window as a modal layer on a grid of `grid_rows` rows; built anew for every event and every
	/// frame.
	pub fn ui<'a>(&'a mut self, locale: &str, grid_rows: u16) -> Ui<'a, HelpMsg> {
		let table_rows = self.table_rows(grid_rows);
		let Self { rows, table, close } = self;
		let rows: &'a [(String, String)] = rows;
		let plain = Style::new().fg(TEXT).bg(BACKGROUND);

		let keys_width = rows.iter().map(|(keys, _)| keys.chars().count()).max().unwrap_or(0);
		let lines = rows
			.iter()
			.map(|(keys, action)| Row::new([Text::styled(keys.as_str(), Style::new().bold()), Text::raw(action.as_str())]));
		let widths = [
			Constraint::Length(u16::try_from(keys_width).unwrap_or(u16::MAX)),
			Constraint::Fill(1),
		];
		let mut bindings = Table::new(lines, widths)
			.column_spacing(COLUMN_GAP)
			.style(plain)
			.row_highlight_style(SELECTED);
		// A table that fits needs no bar.
		if rows.len() > usize::from(table_rows) {
			let bar = Scrollbar::new(ScrollbarOrientation::Vertical)
				.style(Style::new().fg(LABEL))
				.thumb_style(Style::new().fg(TEXT));
			bindings = bindings.scrollbar(bar);
		}

		let close_label = format!("[{}]", t!("help.close", locale = locale));
		let close_width = u16::try_from(close_label.chars().count()).unwrap_or(u16::MAX);
		let button = Button::new(close_label)
			.style(plain)
			.focus_style(Style::new().underlined())
			.pressed_style(SELECTED);
		let button_row = Ui::split(
			Layout::horizontal([Constraint::Fill(1), Constraint::Length(close_width)]),
			vec![
				Ui::widget(Paragraph::new(Line::styled(
					t!("help.hint", locale = locale).into_owned(),
					Style::new().fg(HINT),
				))),
				Ui::stateful(button, close).on_activate(HelpMsg::Close),
			],
		);

		let title = format!(" {} ", t!("help.title", locale = locale));
		let content = Ui::group(
			Block::bordered().title(title).border_style(Style::new().fg(TEXT)),
			Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]),
			vec![Ui::stateful(bindings, table), button_row],
		);
		let layer = Modal::centered(WIDTH, table_rows + CHROME_ROWS)
			.style(plain)
			.content(content)
			.on_close(HelpMsg::Close);
		Ui::split(Layout::vertical([Constraint::Fill(1)]), Vec::new()).modal(layer)
	}

	/// What the controls reported for one input event. `key` is the pressed key when the event was a
	/// key press: Enter and F1 close the window wherever the focus is.
	pub fn settle(&self, routed: Routed<HelpMsg>, key: Option<&Key>) -> HelpOutcome {
		let is_closed_by_a_control = routed.into_iter().any(|message| message == HelpMsg::Close);
		let is_closing_key = matches!(key, Some(Key::Named(NamedKey::Enter | NamedKey::F1)));
		if is_closed_by_a_control || is_closing_key {
			HelpOutcome::Closed
		} else {
			HelpOutcome::Pending
		}
	}
}

#[cfg(test)]
mod tests;
