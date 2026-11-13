//! Snapshot tests of the window dialogs: text and cell colours of a `ScreenBuffer`, no window.

use std::path::{Path, PathBuf};

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
fn the_path_dialog_shows_the_field_with_its_cursor_the_error_and_the_recent_banks() {
	let recent = vec![PathBuf::from("D:/sf2/A.sf2"), PathBuf::from("D:/sf2/B.sf2")];
	let mut input = PathInput::new("D:/sf2/Gen", recent);
	let buf = shown(Modal::Sf2Path(input.clone()));
	assert!(is_shown(&buf, "┌ Файл SoundFont ─"));
	assert!(is_shown(&buf, "│Путь: D:/sf2/Gen"));
	assert!(is_shown(&buf, "│Последние банки:"));
	assert!(is_shown(&buf, "│  D:/sf2/A.sf2"));
	assert!(is_shown(
		&buf,
		"│Tab — дополнить · ↑↓ — недавние · Enter — загрузить · Esc — отмена"
	));
	// The cursor is after the last character: an inverted empty cell.
	let (row, col) = find(&buf, "D:/sf2/Gen");
	let after_text = col + "D:/sf2/Gen".chars().count();
	assert_eq!(buf.cell(after_text, row).bg, TEXT);
	assert_eq!(buf.cell(after_text - 1, row).bg, BACKGROUND);
	assert_eq!(colours_of(&buf, "  D:/sf2/A.sf2"), (TEXT, BACKGROUND));

	// Moved left, the cursor inverts the character it stands on.
	input.handle_key(press(KeyCode::Left), |_| Vec::new());
	let buf = shown(Modal::Sf2Path(input.clone()));
	let under_cursor = buf.cell(after_text - 1, row);
	assert_eq!((under_cursor.ch, under_cursor.fg, under_cursor.bg), ('n', BACKGROUND, TEXT));
	assert_eq!(buf.cell(after_text, row).bg, BACKGROUND);

	input.set_error("file not found: D:/sf2/Gen".into());
	let buf = shown(Modal::Sf2Path(input.clone()));
	let (error_row, _) = find(&buf, "│file not found: D:/sf2/Gen");
	assert_eq!(error_row, row + 1, "the error sits under the field");
	assert_eq!(colours_of(&buf, "file not found").0, ERROR);

	// ↓ walks the recent list: the chosen bank is highlighted and copied into the field.
	input.handle_key(press(KeyCode::Down), |_| Vec::new());
	let buf = shown(Modal::Sf2Path(input));
	assert_eq!(colours_of(&buf, "  D:/sf2/A.sf2"), (BACKGROUND, TEXT));
	assert!(is_shown(&buf, "│Путь: D:/sf2/A.sf2"));

	let empty = shown(Modal::Sf2Path(PathInput::new("", Vec::new())));
	assert!(is_shown(&empty, "│недавних банков нет"));
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
fn the_record_dialog_shows_the_folder_the_format_and_what_is_being_edited() {
	let buf = shown(Modal::Record(RecordDialog::new(None, RecordFormat::Flac, 0.5)));
	assert!(is_shown(&buf, "┌ Запись ─"));
	assert!(is_shown(&buf, "│Каталог: (пусто: папка «Музыка»/sf2_player)"));
	assert!(is_shown(&buf, "│Формат: (●) FLAC   ( ) Ogg Vorbis"));
	assert!(is_shown(&buf, "│без потерь, около 3–5 МБ/мин"));
	assert!(is_shown(
		&buf,
		"│↑↓ — поле · Tab — дополнить · ←→ — формат · Enter — сохранить · Esc — отмена"
	));
	// An empty folder shows the placeholder dimmed, with the cursor on its first cell.
	let (row, col) = find(&buf, "(пусто:");
	assert_eq!(buf.cell(col, row).bg, TEXT);
	assert_eq!((buf.cell(col + 1, row).fg, buf.cell(col + 1, row).bg), (HINT, BACKGROUND));
	assert_eq!(colours_of(&buf, "(●) FLAC"), (TEXT, BACKGROUND));
	assert_eq!(colours_of(&buf, "без потерь").0, LABEL);

	let mut dialog = RecordDialog::new(Some(Path::new("D:/takes")), RecordFormat::OggVorbis, 0.5);
	let buf = shown(Modal::Record(dialog.clone()));
	assert!(is_shown(&buf, "│Каталог: D:/takes"));
	assert!(is_shown(&buf, "│Формат: ( ) FLAC   (●) Ogg Vorbis"));
	assert!(is_shown(&buf, "│с потерями, около 1 МБ/мин, качество 0.5"));
	let (row, col) = find(&buf, "D:/takes");
	assert_eq!(
		buf.cell(col + "D:/takes".chars().count(), row).bg,
		TEXT,
		"the cursor follows the folder"
	);

	// ↓ moves to the format row: it is highlighted and the field loses its cursor.
	dialog.handle_key(press(KeyCode::Down), |_| Vec::new());
	let buf = shown(Modal::Record(dialog));
	assert_eq!(colours_of(&buf, "( ) FLAC"), (BACKGROUND, TEXT));
	assert_eq!(buf.cell(col + "D:/takes".chars().count(), row).bg, BACKGROUND);
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
	assert!(is_shown(&buf, "F10 / Ctrl+C"));
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
