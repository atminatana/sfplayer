//! Drawing the modal windows (spec §6): the list selector and the SoundFont path field.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use rust_i18n::t;

use super::dialog::{Dialog, DialogKind};
use super::keys::help_rows;
use super::path_input::PathInput;
use super::screen::{label, padded};

/// Rows of the list visible at once in a dialog.
const DIALOG_LIST_ROWS: u16 = 10;

/// Modal selector centred over the screen (spec §6 dialog type 1).
pub(super) fn render_dialog(frame: &mut Frame, screen: Rect, dialog: &Dialog, locale: &str, channel: u8) {
	let width = screen.width.saturating_sub(8).clamp(40, 70);
	let height = (DIALOG_LIST_ROWS + 6).min(screen.height.saturating_sub(2));
	let area = Rect {
		x: screen.x + (screen.width - width) / 2,
		y: screen.y + (screen.height - height) / 2,
		width,
		height,
	};
	frame.render_widget(Clear, area);
	let title = match dialog.kind {
		DialogKind::Preset => t!(dialog.kind.title_key(), locale = locale, channel = channel).into_owned(),
		_ => t!(dialog.kind.title_key(), locale = locale).into_owned(),
	};
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Cyan))
		.title(padded(&title));
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let selector = &dialog.selector;
	let visible = selector.visible();
	let list_rows = usize::from(inner.height.saturating_sub(3));
	let first = selector.cursor().saturating_sub(list_rows.saturating_sub(1));
	let mut lines = vec![Line::from(vec![
		label(t!("dialog.filter", locale = locale)),
		Span::raw(format!(" {}_", selector.filter())),
	])];
	if visible.is_empty() {
		let key = if selector.filter().is_empty() {
			"dialog.empty"
		} else {
			"dialog.no_match"
		};
		lines.push(Line::from(Span::styled(
			t!(key, locale = locale).into_owned(),
			Style::default().fg(Color::DarkGray),
		)));
	}
	for (row, index) in visible.iter().enumerate().skip(first).take(list_rows) {
		let item = selector.item(*index);
		let marker = if selector.current() == Some(*index) { "● " } else { "  " };
		let style = if row == selector.cursor() {
			Style::default().add_modifier(Modifier::REVERSED)
		} else {
			Style::default()
		};
		lines.push(Line::from(Span::styled(format!("{marker}{}", item.label), style)));
	}
	while lines.len() < list_rows + 1 {
		lines.push(Line::from(""));
	}
	let detail = selector
		.highlighted()
		.and_then(|index| selector.item(index).detail.clone())
		.unwrap_or_default();
	lines.push(Line::from(Span::styled(detail, Style::default().fg(Color::Gray))));
	lines.push(Line::from(Span::styled(
		t!("dialog.hint", locale = locale).into_owned(),
		Style::default().fg(Color::DarkGray),
	)));
	frame.render_widget(Paragraph::new(lines), inner);
}

/// SoundFont path dialog (spec §6 dialog type 2): editable field with cursor, error line, recent banks.
pub(super) fn render_path_dialog(frame: &mut Frame, screen: Rect, input: &PathInput, locale: &str) {
	let width = screen.width.saturating_sub(8).clamp(40, 76);
	let height = (crate::config::RECENT_BANKS_LIMIT as u16 + 7).min(screen.height.saturating_sub(2));
	let area = Rect {
		x: screen.x + (screen.width - width) / 2,
		y: screen.y + (screen.height - height) / 2,
		width,
		height,
	};
	frame.render_widget(Clear, area);
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Cyan))
		.title(padded(&t!("dialog.sf2_title", locale = locale)));
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let field_label = t!("dialog.sf2_path", locale = locale);
	let mut lines = vec![Line::from(vec![
		label(field_label.clone()),
		Span::raw(format!(" {}", input.text())),
	])];
	lines.push(Line::from(Span::styled(
		input.error().unwrap_or_default().to_owned(),
		Style::default().fg(Color::Red),
	)));
	lines.push(Line::from(label(t!("dialog.sf2_recent", locale = locale))));
	if input.recent().is_empty() {
		lines.push(Line::from(Span::styled(
			t!("dialog.sf2_no_recent", locale = locale).into_owned(),
			Style::default().fg(Color::DarkGray),
		)));
	}
	for (index, path) in input.recent().iter().enumerate() {
		let style = if input.recent_cursor() == Some(index) {
			Style::default().add_modifier(Modifier::REVERSED)
		} else {
			Style::default()
		};
		lines.push(Line::from(Span::styled(format!("  {}", path.display()), style)));
	}
	while lines.len() < usize::from(inner.height.saturating_sub(1)) {
		lines.push(Line::from(""));
	}
	lines.truncate(usize::from(inner.height.saturating_sub(1)));
	lines.push(Line::from(Span::styled(
		t!("dialog.sf2_hint", locale = locale).into_owned(),
		Style::default().fg(Color::DarkGray),
	)));
	frame.render_widget(Paragraph::new(lines), inner);

	let cursor_x = inner.x + u16::try_from(field_label.chars().count() + 1 + input.cursor()).unwrap_or(u16::MAX);
	frame.set_cursor_position((cursor_x.min(inner.x + inner.width.saturating_sub(1)), inner.y));
}

/// F1: the hotkey table, generated from `keys::BINDINGS` so it can never disagree with the handler (ADR 0004).
pub(super) fn render_help(frame: &mut Frame, screen: Rect, scroll: usize, locale: &str) {
	let width = screen.width.saturating_sub(8).clamp(40, 72);
	let height = screen.height.saturating_sub(4).max(8);
	let area = Rect {
		x: screen.x + (screen.width - width) / 2,
		y: screen.y + (screen.height - height) / 2,
		width,
		height,
	};
	frame.render_widget(Clear, area);
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Cyan))
		.title(padded(&t!("help.title", locale = locale)));
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let rows = help_rows(locale);
	let key_width = rows.iter().map(|(keys, _)| keys.chars().count()).max().unwrap_or(0);
	let body_rows = usize::from(inner.height.saturating_sub(1));
	let max_scroll = rows.len().saturating_sub(body_rows);
	let scroll = scroll.min(max_scroll);
	let mut lines: Vec<Line> = rows
		.iter()
		.skip(scroll)
		.take(body_rows)
		.map(|(keys, description)| Line::from(vec![label(format!("{keys:<key_width$}")), Span::raw(format!("  {description}"))]))
		.collect();
	while lines.len() < body_rows {
		lines.push(Line::from(""));
	}
	lines.push(Line::from(Span::styled(
		t!("help.hint", locale = locale).into_owned(),
		Style::default().fg(Color::DarkGray),
	)));
	frame.render_widget(Paragraph::new(lines), inner);
}
