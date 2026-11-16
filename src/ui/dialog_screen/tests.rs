//! Snapshot tests of the window dialogs: text and cell colours of a `ScreenBuffer`, no window.

use gterm::cell::CellFlags;

use super::*;
use crate::audio::AudioDeviceInfo;
use crate::midi::MidiPortInfo;
use crate::session::EffectsSettings;
use crate::ui::chord::{KeyCode, KeyEvent, KeyModifiers};
use crate::ui::paint::BACKGROUND;

const COLS: usize = 96;
const ROWS: usize = 31;
/// What an untouched cell of a `ScreenBuffer` holds.
const EMPTY_CELL: char = char::MIN;

fn screen() -> ScreenBuffer {
	ScreenBuffer::with_default_colors(COLS, ROWS, TEXT, BACKGROUND)
}

fn lines_of(buf: &ScreenBuffer) -> Vec<String> {
	(0..buf.rows())
		.map(|row| {
			let line: String = (0..buf.cols())
				.map(|col| match buf.cell(col, row).ch {
					EMPTY_CELL => ' ',
					ch => ch,
				})
				.collect();
			line.trim_end().to_owned()
		})
		.collect()
}

fn shown_in(locale: &'static str, modal: Modal) -> ScreenBuffer {
	let mut app = App::new(locale);
	app.modal = Some(modal);
	let mut buf = screen();
	render(&mut buf, &app);
	buf
}

fn shown(modal: Modal) -> ScreenBuffer {
	shown_in("ru", modal)
}

/// Row and column (in cells) of the first occurrence of `text`.
fn find(buf: &ScreenBuffer, text: &str) -> (usize, usize) {
	for (row, line) in lines_of(buf).iter().enumerate() {
		if let Some(byte) = line.find(text) {
			return (row, line[..byte].chars().count());
		}
	}
	panic!("{text:?} is not on the screen:\n{}", lines_of(buf).join("\n"));
}

fn is_shown(buf: &ScreenBuffer, text: &str) -> bool {
	lines_of(buf).iter().any(|line| line.contains(text))
}

/// Foreground and background of the first cell of `text`.
fn colours_of(buf: &ScreenBuffer, text: &str) -> (Color, Color) {
	let (row, col) = find(buf, text);
	(buf.cell(col, row).fg, buf.cell(col, row).bg)
}

fn press(code: KeyCode) -> KeyEvent {
	KeyEvent::new(code, KeyModifiers::NONE)
}

fn midi_dialog(names: &[&str], connected: Option<&str>) -> Dialog {
	let ports: Vec<MidiPortInfo> = names
		.iter()
		.enumerate()
		.map(|(index, name)| MidiPortInfo {
			index,
			name: (*name).to_owned(),
		})
		.collect();
	Dialog::midi(&ports, connected)
}

fn three_ports() -> Dialog {
	midi_dialog(&["Arturia KeyStep", "loopMIDI Port", "Roland A-49"], Some("loopMIDI Port"))
}

fn effects_dialog() -> EffectsDialog {
	EffectsDialog::new(EffectsSettings {
		reverb: true,
		chorus: false,
		reverb_level: 40,
		chorus_level: 20,
	})
}

#[test]
fn nothing_is_drawn_without_a_dialog() {
	let mut buf = screen();
	render(&mut buf, &App::new("ru"));
	assert!(lines_of(&buf).iter().all(String::is_empty));
}

#[test]
fn the_selector_marks_the_active_item_and_highlights_the_cursor_row() {
	let buf = shown(Modal::Select(three_ports()));
	let lines = lines_of(&buf);
	let (top, left) = find(&buf, "┌ MIDI-вход ─");
	assert_eq!(left, (COLS - 88) / 2, "the box is centred");
	assert_eq!(lines[top].chars().count(), left + 88);
	assert!(lines[top + 1].contains("│Фильтр:"));
	assert!(lines[top + 2].contains("│  Arturia KeyStep"));
	assert!(lines[top + 3].contains("│● loopMIDI Port"), "the connected port is marked");
	assert!(lines[top + 4].contains("│  Roland A-49"));
	assert!(lines[top + 14].contains("│↑↓ PgUp PgDn — выбор · набор текста — фильтр · Enter — применить · Esc — отмена"));
	assert!(lines[top + 15].contains("└─"), "sixteen rows in all");

	// The cursor starts on the active item; its row is highlighted across the dialog.
	assert_eq!(colours_of(&buf, "● loopMIDI Port"), (BACKGROUND, TEXT));
	assert_eq!(buf.cell(left + 60, top + 3).bg, TEXT, "the highlight spans the row");
	assert_eq!(colours_of(&buf, "  Arturia KeyStep"), (TEXT, BACKGROUND));
	assert_eq!(colours_of(&buf, "↑↓ PgUp"), (HINT, BACKGROUND));
	let label = buf.cell(left + 1, top + 1);
	assert!(label.flags.contains(CellFlags::BOLD), "field labels are bold");

	let mut moved = three_ports();
	moved.selector.handle_key(press(KeyCode::Down));
	let buf = shown(Modal::Select(moved));
	assert_eq!(colours_of(&buf, "  Roland A-49"), (BACKGROUND, TEXT));
	assert_eq!(colours_of(&buf, "● loopMIDI Port"), (TEXT, BACKGROUND));
}

#[test]
fn the_selector_shows_the_filter_the_detail_and_the_empty_states() {
	let mut filtered = three_ports();
	for character in "ro".chars() {
		filtered.selector.handle_key(press(KeyCode::Char(character)));
	}
	let buf = shown(Modal::Select(filtered));
	assert!(is_shown(&buf, "│Фильтр: ro"));
	let (row, col) = find(&buf, "Фильтр: ro");
	assert_eq!(
		buf.cell(col + "Фильтр: ro".chars().count(), row).bg,
		TEXT,
		"a cursor cell follows the filter"
	);
	assert!(is_shown(&buf, "│  Roland A-49"));
	assert!(!is_shown(&buf, "Arturia KeyStep"));

	let mut unmatched = three_ports();
	unmatched.selector.handle_key(press(KeyCode::Char('#')));
	let buf = shown(Modal::Select(unmatched));
	assert!(is_shown(&buf, "│нет совпадений"));
	assert_eq!(colours_of(&buf, "нет совпадений").0, HINT);
	assert!(is_shown(&shown(Modal::Select(midi_dialog(&[], None))), "│выбирать нечего"));

	let devices = vec![
		AudioDeviceInfo {
			index: 0,
			name: "Speakers".into(),
			is_default: true,
			default_sample_rate: 48_000,
			buffer_range: Some((128, 2048)),
		},
		AudioDeviceInfo {
			index: 1,
			name: "HDMI".into(),
			is_default: false,
			default_sample_rate: 44_100,
			buffer_range: None,
		},
	];
	let buf = shown_in("en", Modal::Select(Dialog::audio(&devices, Some("Speakers"), "en")));
	assert!(is_shown(&buf, "┌ Audio output ─"));
	let (detail_row, _) = find(&buf, "│48000 Hz, buffer 128..2048 frames");
	let (hint_row, _) = find(&buf, "type to filter");
	assert_eq!(detail_row + 1, hint_row, "the detail of the highlighted item sits above the hint");
	assert_eq!(colours_of(&buf, "48000 Hz, buffer").0, LABEL);
}

#[test]
fn a_long_selector_list_scrolls_to_keep_the_cursor_visible() {
	let names: Vec<String> = (1..=30).map(|number| format!("Port {number:02}")).collect();
	let names: Vec<&str> = names.iter().map(String::as_str).collect();
	let mut long = midi_dialog(&names, None);
	let buf = shown(Modal::Select(long.clone()));
	assert!(is_shown(&buf, "Port 01") && is_shown(&buf, "Port 11"));
	assert!(!is_shown(&buf, "Port 12"), "eleven list rows fit");

	for _ in 0..20 {
		long.selector.handle_key(press(KeyCode::Down));
	}
	let buf = shown(Modal::Select(long));
	assert_eq!(
		colours_of(&buf, "  Port 21"),
		(BACKGROUND, TEXT),
		"the cursor row is the last visible one"
	);
	assert!(is_shown(&buf, "Port 11") && !is_shown(&buf, "Port 10") && !is_shown(&buf, "Port 22"));
}

#[test]
fn the_effects_dialog_shows_switches_levels_and_the_current_row() {
	let mut dialog = effects_dialog();
	let buf = shown(Modal::Effects(dialog.clone()));
	assert!(is_shown(&buf, "┌ Эффекты ─"));
	assert!(is_shown(&buf, "│Реверб   [x]  уровень  40"));
	assert!(is_shown(&buf, "│Хорус    [ ]  уровень  20"));
	assert!(is_shown(
		&buf,
		"│↑↓ — строка · Space — вкл/выкл · ←→ — уровень ±5 · Enter — сохранить · Esc — отмена"
	));
	assert_eq!(colours_of(&buf, "Реверб   [x]"), (BACKGROUND, TEXT));
	assert_eq!(colours_of(&buf, "Хорус    [ ]"), (TEXT, BACKGROUND));

	dialog.handle_key(press(KeyCode::Down));
	dialog.handle_key(press(KeyCode::Char(' ')));
	dialog.handle_key(press(KeyCode::Right));
	let buf = shown(Modal::Effects(dialog));
	assert!(is_shown(&buf, "│Хорус    [x]  уровень  25"));
	assert_eq!(colours_of(&buf, "Хорус    [x]"), (BACKGROUND, TEXT));
	assert_eq!(colours_of(&buf, "Реверб   [x]"), (TEXT, BACKGROUND));

	let english = shown_in("en", Modal::Effects(effects_dialog()));
	assert!(is_shown(&english, "│Reverb   [x]  level  40"));
}

#[test]
fn the_quit_confirmation_is_framed_in_red() {
	let buf = shown(Modal::ConfirmQuit);
	assert!(is_shown(&buf, "┌ Идёт запись ─"));
	assert!(is_shown(&buf, "│Остановить запись и выйти?"));
	assert!(is_shown(&buf, "│Enter — остановить и выйти · Esc — продолжить запись"));
	assert_eq!(colours_of(&buf, "┌").0, ERROR);
	let (top, left) = find(&buf, "┌ Идёт запись ─");
	assert_eq!(lines_of(&buf)[top].chars().count() - left, 60);
	assert_eq!(colours_of(&shown(Modal::Select(three_ports())), "┌").0, TEXT);
}

#[test]
fn the_help_lists_the_bindings_and_scrolls() {
	let buf = shown(Modal::Help(0));
	assert!(is_shown(&buf, "┌ Справка ─"));
	let (first_row, col) = find(&buf, "F1 ");
	assert!(lines_of(&buf)[first_row].contains("Эта справка"));
	assert!(buf.cell(col, first_row).flags.contains(CellFlags::BOLD), "the key column is bold");
	assert!(is_shown(&buf, "+ / ="), "the chords of one command share a row");
	assert!(is_shown(&buf, "│↑↓ PgUp PgDn — прокрутка · Esc или F1 — закрыть"));
	assert!(!is_shown(&buf, "Z X C V B N M"), "the last rows are below the fold");

	// The table is the bindings table: every row of it is reachable by scrolling.
	let rows = Bindings::default().help_rows("ru");
	let last = &rows[rows.len() - 1].1;
	let scrolled = shown(Modal::Help(usize::MAX));
	assert!(is_shown(&scrolled, last), "scrolling stops at the end of the table");
	assert!(!is_shown(&scrolled, "Эта справка"));
	let one_down = shown(Modal::Help(1));
	let (row, _) = find(&one_down, "F2 ");
	assert_eq!(row, first_row, "one step scrolls one row");
}

#[test]
fn a_dialog_covers_only_its_own_box() {
	// Fill the screen with a pattern, draw each dialog over it, and compare everything outside the frame.
	let pattern = "#".repeat(COLS);
	for modal in [
		Modal::Select(three_ports()),
		Modal::Effects(effects_dialog()),
		Modal::ConfirmQuit,
		Modal::Help(0),
	] {
		let mut app = App::new("ru");
		app.modal = Some(modal);
		let mut buf = screen();
		for row in 0..ROWS {
			buf.write_row(row, &pattern, TEXT, BACKGROUND);
		}
		render(&mut buf, &app);
		let (top, left) = find(&buf, "┌");
		let (bottom, _) = find(&buf, "└");
		let (_, right) = find(&buf, "┐");
		for row in 0..ROWS {
			for col in 0..COLS {
				let is_inside = (top..=bottom).contains(&row) && (left..=right).contains(&col);
				if !is_inside {
					assert_eq!(buf.cell(col, row).ch, '#', "cell {col},{row} outside the dialog was touched");
				}
			}
		}
		assert_ne!(buf.cell(left + 1, top + 2).ch, '#', "the inside is cleared");
	}
}

#[test]
fn dialogs_fit_a_grid_smaller_than_they_would_like() {
	let mut app = App::new("ru");
	app.modal = Some(Modal::Help(0));
	let mut small = ScreenBuffer::with_default_colors(30, 6, TEXT, BACKGROUND);
	render(&mut small, &app);
	let lines = lines_of(&small);
	assert!(lines[0].starts_with("┌ Справка ─"), "the box shrinks to the grid: {lines:?}");
	assert!(lines[5].starts_with("└─"));
	assert_eq!(lines[0].chars().count(), 30);
}

#[test]
fn the_reported_area_is_exactly_the_frame_that_is_drawn() {
	let grid = Rect::of_screen(&screen());
	assert_eq!(area(&App::new("ru"), grid), None, "no dialog, no area");
	for modal in [
		Modal::Select(midi_dialog(&["KeyStep", "loopMIDI"], None)),
		Modal::Effects(effects_dialog()),
		Modal::Help(0),
		Modal::ConfirmQuit,
	] {
		let mut app = App::new("ru");
		app.modal = Some(modal.clone());
		let area = area(&app, grid).expect("a dialog is open");
		let buf = shown(modal.clone());
		let corner = |col: u16, row: u16| buf.cell(usize::from(col), usize::from(row)).ch;
		let (right, bottom) = (area.x + area.width - 1, area.y + area.height - 1);
		assert_eq!(
			[
				corner(area.x, area.y),
				corner(right, area.y),
				corner(area.x, bottom),
				corner(right, bottom)
			],
			['┌', '┐', '└', '┘'],
			"{modal:?}"
		);
		// Nothing of the dialog lies outside the area.
		for row in 0..grid.height {
			for col in 0..grid.width {
				assert!(
					area.contains(col, row) || corner(col, row) == EMPTY_CELL,
					"{modal:?}: cell {col},{row}"
				);
			}
		}
	}
}
