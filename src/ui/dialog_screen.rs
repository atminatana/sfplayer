//! Drawing the modal windows (spec §6, v1.1 §4): the list selector, the effects and recording dialogs,
//! the help table and the quit confirmation.
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
use super::record_dialog::{RECORD_FORMATS, RecordDialog, RecordField};
use super::view::format_name;
use crate::config::RecordFormat;

/// Rows of the list visible at once in a selector.
const SELECTOR_LIST_ROWS: u16 = 10;
/// Columns kept free on both sides of a dialog, and the narrowest a dialog gets.
const SIDE_MARGIN: u16 = 8;
const MIN_WIDTH: u16 = 40;
/// The widest a dialog with a one-line key hint gets: the longest hint fits on the minimum grid.
const WIDE: u16 = 88;
/// The quit confirmation is a short question.
const NARROW: u16 = 60;
/// Marks the item that is active now in a selector, and the chosen radio option.
const CURRENT_MARK: &str = "●";

/// Draws the open dialog, if any, over what is already in `buf`.
pub fn render(buf: &mut ScreenBuffer, app: &App) {
	let locale = app.locale;
	match &app.modal {
		Some(Modal::Select(dialog)) => selector(buf, dialog, locale),
		// The window shows the path dialog as the input dialog on widget controls (`input_dialog`).
		Some(Modal::Sf2Path(_)) | None => {}
		Some(Modal::Effects(dialog)) => effects(buf, dialog, locale),
		Some(Modal::Record(dialog)) => record(buf, dialog, locale),
		Some(Modal::Help(scroll)) => help(buf, &app.bindings, *scroll, locale),
		Some(Modal::ConfirmQuit) => confirm_quit(buf, locale),
	}
}

/// Clears a centred box, frames it with the title in the top border, and returns the area inside.
fn frame(buf: &mut ScreenBuffer, max_width: u16, height: u16, title: &str, border: Color) -> Rect {
	let screen = buf.area();
	let width = screen
		.width
		.saturating_sub(SIDE_MARGIN)
		.clamp(MIN_WIDTH, max_width)
		.min(screen.width);
	let area = screen.centered(width, height.min(screen.height));
	Clear.render(area, buf);
	let block = Block::bordered().title(format!(" {title} ")).border_style(Style::new().fg(border));
	let inner = block.inner(area);
	block.render(area, buf);
	inner
}

fn bold_label() -> Style {
	Style::new().fg(TEXT).bold()
}

/// Draws `text` as an editable field: the cell under the cursor is shown inverted.
fn field(buf: &mut ScreenBuffer, area: Rect, column: u16, row: u16, text: &str, cursor: Option<usize>) {
	Pen::within(buf, area, column, row).text(text, TEXT);
	if let Some(cursor) = cursor {
		let under_cursor = text.chars().nth(cursor).unwrap_or(' ');
		let offset = u16::try_from(cursor).unwrap_or(u16::MAX);
		Pen::within(buf, area, column.saturating_add(offset), row).styled(&under_cursor.to_string(), SELECTED);
	}
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
fn selector(buf: &mut ScreenBuffer, dialog: &Dialog, locale: &str) {
	let title = t!(dialog.kind.title_key(), locale = locale);
	let inner = frame(buf, WIDE, SELECTOR_LIST_ROWS + 6, &title, TEXT);
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
fn effects(buf: &mut ScreenBuffer, dialog: &EffectsDialog, locale: &str) {
	let inner = frame(buf, WIDE, 6, &t!("dialog.effects_title", locale = locale), TEXT);
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

/// F7 (spec v1.1 §4.3): folder field, format options, description of the chosen format.
fn record(buf: &mut ScreenBuffer, dialog: &RecordDialog, locale: &str) {
	let inner = frame(buf, WIDE, 8, &t!("dialog.record_title", locale = locale), TEXT);
	let label = t!("dialog.record_dir", locale = locale);
	Pen::within(buf, inner, 0, 0).styled(&label, bold_label());
	let field_column = u16::try_from(label.chars().count() + 1).unwrap_or(u16::MAX);
	let directory = dialog.directory();
	let is_editing = dialog.focus() == RecordField::Directory;
	let text = directory.text();
	if text.is_empty() {
		// An empty field shows what it means as a placeholder; the cursor sits on its first cell.
		let placeholder = t!("dialog.record_dir_default", locale = locale);
		Pen::within(buf, inner, field_column, 0).text(&placeholder, HINT);
		if is_editing {
			let first: String = placeholder.chars().take(1).collect();
			Pen::within(buf, inner, field_column, 0).styled(&first, SELECTED);
		}
	} else {
		field(buf, inner, field_column, 0, &text, is_editing.then(|| directory.cursor()));
	}

	let options = RECORD_FORMATS
		.iter()
		.map(|format| {
			let mark = if *format == dialog.format() { CURRENT_MARK } else { " " };
			format!("({mark}) {}", format_name(*format))
		})
		.collect::<Vec<_>>()
		.join("   ");
	let mut pen = Pen::within(buf, inner, 0, 2);
	pen.styled(&t!("dialog.record_format", locale = locale), bold_label()).gap(1);
	if dialog.focus() == RecordField::Format {
		pen.styled(&options, SELECTED);
	} else {
		pen.text(&options, TEXT);
	}
	let description = match dialog.format() {
		RecordFormat::Flac => t!("dialog.record_flac", locale = locale),
		RecordFormat::OggVorbis => t!("dialog.record_vorbis", locale = locale, quality = dialog.vorbis_quality()),
	};
	Pen::within(buf, inner, 0, 3).text(&description, LABEL);
	hint(buf, inner, &t!("dialog.record_hint", locale = locale));
}

/// Spec §6: quitting while a recording runs asks first. Enter stops the take and quits, Esc stays.
fn confirm_quit(buf: &mut ScreenBuffer, locale: &str) {
	let inner = frame(buf, NARROW, 5, &t!("confirm.quit_title", locale = locale), ERROR);
	Pen::within(buf, inner, 0, 0).text(&t!("confirm.quit_text", locale = locale), TEXT);
	hint(buf, inner, &t!("confirm.quit_hint", locale = locale));
}

/// F1: the hotkey table, generated from the bindings so it can never disagree with the handler (ADR 0004).
fn help(buf: &mut ScreenBuffer, bindings: &Bindings, scroll: usize, locale: &str) {
	let height = buf.area().height.saturating_sub(4).max(8);
	let inner = frame(buf, WIDE, height, &t!("help.title", locale = locale), TEXT);
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
