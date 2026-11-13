//! Drawing the modal windows (spec §6, v1.1 §4): the list selector, the effects dialog, the help table
//! and the quit confirmation.
//!
//! The dialogs themselves are state machines kept in `App::modal`; this module only shows them, as a
//! framed box over the middle of the main screen.

use gterm::buffer::ScreenBuffer;
use gterm::cell::Color;
use gterm_widgets::widgets::{Block, Clear};
use gterm_widgets::{BufferExt, Rect, Style, Widget};
use rust_i18n::t;

use super::app::{App, Modal};
use super::dialog::Dialog;
use super::effects_dialog::{EffectRow, EffectsDialog};
use super::keys::Bindings;
use super::paint::{ERROR, HINT, LABEL, Pen, SELECTED, TEXT};

/// Rows of the list visible at once in a selector.
const SELECTOR_LIST_ROWS: u16 = 10;
/// Columns kept free on both sides of a dialog, and the narrowest a dialog gets.
const SIDE_MARGIN: u16 = 8;
const MIN_WIDTH: u16 = 40;
/// The widest a dialog with a one-line key hint gets: the longest hint fits on the minimum grid.
const WIDE: u16 = 88;
/// The quit confirmation is a short question.
const NARROW: u16 = 60;
/// Marks the item that is active now in a selector.
const CURRENT_MARK: &str = "●";

/// Draws the open dialog, if any, over what is already in `buf`.
pub fn render(buf: &mut ScreenBuffer, app: &App) {
	let locale = app.locale;
	let Some(modal) = &app.modal else {
		return;
	};
	let area = area_of(modal, buf.area());
	match modal {
		Modal::Select(dialog) => selector(buf, area, dialog, locale),
		Modal::Effects(dialog) => effects(buf, area, dialog, locale),
		Modal::Help(scroll) => help(buf, area, &app.bindings, *scroll, locale),
		Modal::ConfirmQuit => confirm_quit(buf, area, locale),
	}
}

/// The cells the open dialog covers on a grid of `screen`; a click outside them closes the dialog.
pub fn area(app: &App, screen: Rect) -> Option<Rect> {
	app.modal.as_ref().map(|modal| area_of(modal, screen))
}

/// A dialog sits in the middle of the grid, as wide as its kind allows and the grid leaves room for.
fn area_of(modal: &Modal, screen: Rect) -> Rect {
	let (max_width, height) = match modal {
		Modal::Select(_) => (WIDE, SELECTOR_LIST_ROWS + 6),
		Modal::Effects(_) => (WIDE, 6),
		Modal::Help(_) => (WIDE, screen.height.saturating_sub(4).max(8)),
		Modal::ConfirmQuit => (NARROW, 5),
	};
	let width = screen
		.width
		.saturating_sub(SIDE_MARGIN)
		.clamp(MIN_WIDTH, max_width)
		.min(screen.width);
	screen.centered(width, height.min(screen.height))
}

/// Clears the box, frames it with the title in the top border, and returns the area inside.
fn frame(buf: &mut ScreenBuffer, area: Rect, title: &str, border: Color) -> Rect {
	Clear.render(area, buf);
	let block = Block::bordered().title(format!(" {title} ")).border_style(Style::new().fg(border));
	let inner = block.inner(area);
	block.render(area, buf);
	inner
}

fn bold_label() -> Style {
	Style::new().fg(TEXT).bold()
}

/// A list row: plain, or highlighted across the whole width of the dialog.
fn list_row(buf: &mut ScreenBuffer, area: Rect, row: u16, text: &str, is_highlighted: bool) {
	if is_highlighted {
		buf.set_style(Rect::new(area.x, area.y.saturating_add(row), area.width, 1), SELECTED);
		Pen::within(buf, area, 0, row).styled(text, SELECTED);
	} else {
		Pen::within(buf, area, 0, row).text(text, TEXT);
	}
}

fn hint(buf: &mut ScreenBuffer, area: Rect, text: &str) {
	Pen::within(buf, area, 0, area.height.saturating_sub(1)).text(text, HINT);
}

/// Modal selector (spec §6 dialog type 1): filter, list with the active item marked, detail, hint.
fn selector(buf: &mut ScreenBuffer, area: Rect, dialog: &Dialog, locale: &str) {
	let title = t!(dialog.kind.title_key(), locale = locale);
	let inner = frame(buf, area, &title, TEXT);
	let selector = &dialog.selector;
	let visible = selector.visible();
	let list_rows = usize::from(inner.height.saturating_sub(3));
	let first = selector.cursor().saturating_sub(list_rows.saturating_sub(1));

	Pen::within(buf, inner, 0, 0)
		.styled(&t!("dialog.filter", locale = locale), bold_label())
		.gap(1)
		.text(selector.filter(), TEXT)
		.styled(" ", SELECTED);
	if visible.is_empty() {
		let key = if selector.filter().is_empty() {
			"dialog.empty"
		} else {
			"dialog.no_match"
		};
		Pen::within(buf, inner, 0, 1).text(&t!(key, locale = locale), HINT);
	}
	for (line, (position, index)) in visible.iter().enumerate().skip(first).take(list_rows).enumerate() {
		let item = selector.item(*index);
		let mark = if selector.current() == Some(*index) { CURRENT_MARK } else { " " };
		let row = u16::try_from(line + 1).unwrap_or(u16::MAX);
		list_row(buf, inner, row, &format!("{mark} {}", item.label), position == selector.cursor());
	}
	if let Some(detail) = selector.highlighted().and_then(|index| selector.item(index).detail.as_deref()) {
		Pen::within(buf, inner, 0, inner.height.saturating_sub(2)).text(detail, LABEL);
	}
	hint(buf, inner, &t!("dialog.hint", locale = locale));
}

/// F6 (spec v1.1 §4.2): one row per effect — switch and send level; the current row is highlighted.
fn effects(buf: &mut ScreenBuffer, area: Rect, dialog: &EffectsDialog, locale: &str) {
	let inner = frame(buf, area, &t!("dialog.effects_title", locale = locale), TEXT);
	let settings = dialog.settings();
	let level_label = t!("dialog.effects_level", locale = locale);
	for (row, effect, name, is_on, level) in [
		(
			0,
			EffectRow::Reverb,
			t!("dialog.effects_reverb", locale = locale),
			settings.reverb,
			settings.reverb_level,
		),
		(
			1,
			EffectRow::Chorus,
			t!("dialog.effects_chorus", locale = locale),
			settings.chorus,
			settings.chorus_level,
		),
	] {
		let switch = if is_on { "[x]" } else { "[ ]" };
		let text = format!("{name:<8} {switch}  {level_label} {level:>3}");
		list_row(buf, inner, row, &text, dialog.row() == effect);
	}
	hint(buf, inner, &t!("dialog.effects_hint", locale = locale));
}

/// Spec §6: quitting while a recording runs asks first. Enter stops the take and quits, Esc stays.
fn confirm_quit(buf: &mut ScreenBuffer, area: Rect, locale: &str) {
	let inner = frame(buf, area, &t!("confirm.quit_title", locale = locale), ERROR);
	Pen::within(buf, inner, 0, 0).text(&t!("confirm.quit_text", locale = locale), TEXT);
	hint(buf, inner, &t!("confirm.quit_hint", locale = locale));
}

/// F1: the hotkey table, generated from the bindings so it can never disagree with the handler (ADR 0004).
fn help(buf: &mut ScreenBuffer, area: Rect, bindings: &Bindings, scroll: usize, locale: &str) {
	let inner = frame(buf, area, &t!("help.title", locale = locale), TEXT);
	let rows = bindings.help_rows(locale);
	let key_width = rows.iter().map(|(keys, _)| keys.chars().count()).max().unwrap_or(0);
	let body_rows = usize::from(inner.height.saturating_sub(1));
	let scroll = scroll.min(rows.len().saturating_sub(body_rows));
	for (line, (keys, description)) in rows.iter().skip(scroll).take(body_rows).enumerate() {
		let row = u16::try_from(line).unwrap_or(u16::MAX);
		Pen::within(buf, inner, 0, row)
			.styled(&format!("{keys:<key_width$}"), bold_label())
			.gap(2)
			.text(description, TEXT);
	}
	hint(buf, inner, &t!("help.hint", locale = locale));
}

#[cfg(test)]
mod tests;
