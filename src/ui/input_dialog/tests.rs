//! Tests of the input dialog: keys and the mouse go through the same routing entry points the window
//! uses, the picture is read back from the modal layer's buffer. No window, no file system.

use gterm::buffer::ScreenBuffer;
use gterm::cell::Color;
use gterm_widgets::{KeyMsg, MemoryClipboard, MouseKind, MouseMsg, Rect};
use winit::event::MouseButton;
use winit::keyboard::{ModifiersState, SmolStr};

use super::*;

const SCREEN: Rect = Rect::new(0, 0, 96, 31);
/// What an untouched cell of a `ScreenBuffer` holds.
const EMPTY_CELL: char = char::MIN;
const SEP: char = MAIN_SEPARATOR;

/// A small fake tree: `D:/sf2/` with a folder, two banks, and files that are not banks.
fn fake_dir(path: &Path) -> Vec<DirEntry> {
	let entry = |name: &str, is_dir: bool| DirEntry {
		name: name.to_owned(),
		is_dir,
	};
	match path.to_string_lossy().as_ref() {
		"D:/sf2/" => vec![
			entry("Banks", true),
			entry("GeneralUser.sf2", false),
			entry("Grand.SF2", false),
			entry("gm.sf3", false),
			entry("notes.txt", false),
		],
		"D:/sf2/Banks/" | "D:/sf2/Banks\\" => vec![entry("Organ.sf2", false)],
		"." => vec![entry("local.sf2", false), entry("src", true)],
		_ => Vec::new(),
	}
}

fn history() -> Vec<PathBuf> {
	vec![PathBuf::from("D:/sf2/A.sf2"), PathBuf::from("D:/old/B.sf2")]
}

fn dialog(initial: &str) -> InputDialog {
	InputDialog::new(InputKind::Sf2File, initial, &history(), fake_dir)
}

/// One key press, routed and settled the way the window does it.
fn press_with(dialog: &mut InputDialog, key: &Key, text: Option<&str>) -> InputOutcome {
	let mut clipboard = MemoryClipboard::new();
	let message = KeyMsg {
		key,
		text,
		modifiers: ModifiersState::empty(),
	};
	let routed = dialog.ui("ru", &mut clipboard).route_key(&message);
	dialog.settle(routed, Some(key), fake_dir)
}

fn press(dialog: &mut InputDialog, named: NamedKey) -> InputOutcome {
	press_with(dialog, &Key::Named(named), None)
}

fn type_text(dialog: &mut InputDialog, text: &str) {
	for character in text.chars() {
		let typed = character.to_string();
		let outcome = press_with(dialog, &Key::Character(SmolStr::new(&typed)), Some(&typed));
		assert_eq!(outcome, InputOutcome::Pending, "typing {character:?} keeps the dialog open");
	}
}

/// The modal layer as drawn: where it sits on the screen and its cells.
fn drawn(dialog: &mut InputDialog, locale: &str) -> (Rect, ScreenBuffer) {
	let mut clipboard = MemoryClipboard::new();
	dialog
		.ui(locale, &mut clipboard)
		.modal_buffer(SCREEN)
		.expect("the dialog is a modal layer")
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

/// Row and column of `text` inside the layer.
fn find(buf: &ScreenBuffer, text: &str) -> (usize, usize) {
	for (row, line) in lines_of(buf).iter().enumerate() {
		if let Some(byte) = line.find(text) {
			return (row, line[..byte].chars().count());
		}
	}
	panic!("{text:?} is not in the dialog:\n{}", lines_of(buf).join("\n"));
}

fn colours_of(buf: &ScreenBuffer, text: &str) -> (Color, Color) {
	let (row, col) = find(buf, text);
	(buf.cell(col, row).fg, buf.cell(col, row).bg)
}

/// A left click (press and release) on the first cell of `text`, in screen coordinates.
fn click(dialog: &mut InputDialog, text: &str) -> InputOutcome {
	let (area, buf) = drawn(dialog, "ru");
	let (row, col) = find(&buf, text);
	let col = i32::from(area.x) + i32::try_from(col).expect("column");
	let row = i32::from(area.y) + i32::try_from(row).expect("row");
	click_at(dialog, col, row)
}

fn click_at(dialog: &mut InputDialog, col: i32, row: i32) -> InputOutcome {
	let mut outcome = InputOutcome::Pending;
	for kind in [MouseKind::Down, MouseKind::Up] {
		let mut clipboard = MemoryClipboard::new();
		let message = MouseMsg {
			kind,
			button: Some(MouseButton::Left),
			col,
			row,
			modifiers: ModifiersState::empty(),
		};
		let routed = dialog.ui("ru", &mut clipboard).route_mouse(&message, SCREEN);
		let step = dialog.settle(routed, None, fake_dir);
		if step != InputOutcome::Pending {
			outcome = step;
		}
	}
	outcome
}

#[test]
fn an_empty_field_lists_the_history_and_the_frame_shows_field_list_and_buttons() {
	let mut dialog = dialog("");
	assert_eq!(dialog.rows(), ["D:/sf2/A.sf2", "D:/old/B.sf2"]);
	assert_eq!(dialog.highlighted(), None);

	let (area, buf) = drawn(&mut dialog, "ru");
	assert_eq!((area.width, area.height), (WIDTH, HEIGHT));
	assert_eq!(area.x, (SCREEN.width - WIDTH) / 2, "the dialog is centred");
	let lines = lines_of(&buf);
	assert!(lines[0].starts_with("┌ Файл SoundFont ─"));
	assert!(lines[1].starts_with("│Путь:"));
	assert_eq!(
		lines[2],
		format!("│{}│", " ".repeat(usize::from(WIDTH) - 2)),
		"the error row is empty"
	);
	assert!(lines[3].starts_with("│D:/sf2/A.sf2"));
	assert!(lines[4].starts_with("│D:/old/B.sf2"));
	let buttons = &lines[usize::from(HEIGHT) - 2];
	assert!(buttons.starts_with("│↑↓ — выбор · Enter — открыть · Tab — дальше · Esc — отмена"));
	assert!(buttons.ends_with("[OK] [Отмена]│"), "{buttons}");
	assert!(lines[usize::from(HEIGHT) - 1].starts_with("└─"));
	assert_eq!(colours_of(&buf, "↑↓ — выбор").0, HINT);
	assert_eq!(colours_of(&buf, "D:/sf2/A.sf2"), (TEXT, BACKGROUND), "nothing is highlighted yet");

	let (_, english) = drawn(&mut dialog, "en");
	assert!(lines_of(&english)[usize::from(HEIGHT) - 2].ends_with("[OK] [Cancel]│"));
	assert!(lines_of(&english)[1].starts_with("│Path:"));
}

#[test]
fn typing_lists_matching_folders_and_banks_and_clearing_brings_the_history_back() {
	let mut dialog = dialog("");
	type_text(&mut dialog, "D:/sf2/");
	assert_eq!(dialog.text(), "D:/sf2/");
	// Folders and .sf2 files only, whatever the case of the extension; other files are not offered.
	assert_eq!(
		dialog.rows(),
		[
			format!("D:/sf2/Banks{SEP}"),
			"D:/sf2/GeneralUser.sf2".to_owned(),
			"D:/sf2/Grand.SF2".to_owned()
		]
	);

	type_text(&mut dialog, "g");
	assert_eq!(
		dialog.rows(),
		["D:/sf2/GeneralUser.sf2", "D:/sf2/Grand.SF2"],
		"the prefix ignores case"
	);
	type_text(&mut dialog, "r");
	assert_eq!(dialog.rows(), ["D:/sf2/Grand.SF2"]);
	type_text(&mut dialog, "x");
	assert!(dialog.rows().is_empty(), "nothing matches");
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(lines_of(&buf)[1].starts_with("│Путь: D:/sf2/grx"));

	for _ in 0.."D:/sf2/grx".len() {
		press(&mut dialog, NamedKey::Backspace);
	}
	assert_eq!(dialog.text(), "");
	assert_eq!(
		dialog.rows(),
		["D:/sf2/A.sf2", "D:/old/B.sf2"],
		"an empty field shows the history again"
	);

	// A bare name is looked up in the current directory.
	type_text(&mut dialog, "l");
	assert_eq!(dialog.rows(), ["local.sf2"]);
}

#[test]
fn arrows_walk_the_list_from_the_field_and_put_the_row_into_it() {
	let mut dialog = dialog("");
	type_text(&mut dialog, "D:/sf2/G");
	assert_eq!(press(&mut dialog, NamedKey::ArrowDown), InputOutcome::Pending);
	assert_eq!(dialog.highlighted(), Some(0));
	assert_eq!(dialog.text(), "D:/sf2/GeneralUser.sf2");
	// The list is not filtered again by what was taken from it: the arrows keep walking it.
	assert_eq!(dialog.rows(), ["D:/sf2/GeneralUser.sf2", "D:/sf2/Grand.SF2"]);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert_eq!(
		colours_of(&buf, "D:/sf2/GeneralUser.sf2").1,
		BACKGROUND,
		"the field keeps its colours"
	);
	let (row, col) = find(&buf, "│D:/sf2/GeneralUser.sf2");
	assert_eq!(buf.cell(col + 1, row).bg, TEXT, "the picked row is highlighted in the list");

	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(1), "D:/sf2/Grand.SF2"));
	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(dialog.highlighted(), Some(1), "the list ends there");
	press(&mut dialog, NamedKey::ArrowUp);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(0), "D:/sf2/GeneralUser.sf2"));

	// ↑ on a fresh list starts from the bottom; with no rows the arrows do nothing.
	let mut fresh = self::dialog("");
	press(&mut fresh, NamedKey::ArrowUp);
	assert_eq!((fresh.highlighted(), fresh.text()), (Some(1), "D:/old/B.sf2"));
	let mut empty = InputDialog::new(InputKind::Sf2File, "", &[], fake_dir);
	assert_eq!(press(&mut empty, NamedKey::ArrowDown), InputOutcome::Pending);
	assert_eq!((empty.highlighted(), empty.text()), (None, ""));
}

#[test]
fn enter_submits_a_file_opens_a_folder_and_ignores_an_empty_field() {
	let mut dialog = dialog("");
	assert_eq!(press(&mut dialog, NamedKey::Enter), InputOutcome::Pending, "nothing to submit");

	type_text(&mut dialog, "D:/sf2/");
	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(dialog.text(), format!("D:/sf2/Banks{SEP}"));
	// Enter on a folder shows what is inside instead of submitting it.
	assert_eq!(press(&mut dialog, NamedKey::Enter), InputOutcome::Pending);
	assert_eq!(dialog.rows(), [format!("D:/sf2/Banks{SEP}Organ.sf2")]);
	assert_eq!(dialog.highlighted(), None);

	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(
		press(&mut dialog, NamedKey::Enter),
		InputOutcome::Submit(PathBuf::from(format!("D:/sf2/Banks{SEP}Organ.sf2")))
	);

	// A typed path is submitted as it is, trimmed; the list need not know it.
	let mut typed = self::dialog("  D:/elsewhere/x.sf2 ");
	assert_eq!(
		press(&mut typed, NamedKey::Enter),
		InputOutcome::Submit(PathBuf::from("D:/elsewhere/x.sf2"))
	);
}

#[test]
fn tab_moves_the_focus_through_field_list_and_buttons() {
	let mut dialog = dialog("D:/sf2/G");
	// Field → list: the arrows now belong to the list control, which reports the highlighted row.
	// No frame is drawn between these keys, so the list still has the row count of its last frame
	// (none): the dialog brings what it reports back into range.
	press(&mut dialog, NamedKey::Tab);
	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(0), "D:/sf2/GeneralUser.sf2"));
	press(&mut dialog, NamedKey::End);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(1), "D:/sf2/Grand.SF2"));
	// With a frame in between, as in the running window, the list navigates by its real rows.
	drawn(&mut dialog, "ru");
	press(&mut dialog, NamedKey::Home);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(0), "D:/sf2/GeneralUser.sf2"));
	drawn(&mut dialog, "ru");
	press(&mut dialog, NamedKey::End);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(1), "D:/sf2/Grand.SF2"));
	// Enter in the list accepts what is in the field.
	assert_eq!(
		press(&mut dialog.clone(), NamedKey::Enter),
		InputOutcome::Submit(PathBuf::from("D:/sf2/Grand.SF2"))
	);

	// List → [OK]: the focused button is underlined; Enter presses it.
	press(&mut dialog, NamedKey::Tab);
	let (_, buf) = drawn(&mut dialog, "ru");
	let (row, col) = find(&buf, "[OK]");
	assert!(buf.cell(col, row).flags.contains(gterm::cell::CellFlags::UNDERLINE));
	let (cancel_row, cancel_col) = find(&buf, "[Отмена]");
	assert!(!buf.cell(cancel_col, cancel_row).flags.contains(gterm::cell::CellFlags::UNDERLINE));
	assert_eq!(
		press(&mut dialog.clone(), NamedKey::Enter),
		InputOutcome::Submit(PathBuf::from("D:/sf2/Grand.SF2"))
	);

	// [OK] → [Cancel] → back to the field.
	press(&mut dialog, NamedKey::Tab);
	assert_eq!(press(&mut dialog.clone(), NamedKey::Enter), InputOutcome::Cancelled);
	press(&mut dialog, NamedKey::Tab);
	type_text(&mut dialog, "x");
	assert_eq!(dialog.text(), "D:/sf2/Grand.SF2x", "the focus is back in the field");
}

#[test]
fn escape_and_a_click_outside_cancel() {
	let mut dialog = dialog("D:/sf2/G");
	assert_eq!(press(&mut dialog, NamedKey::Escape), InputOutcome::Cancelled);
	let mut dialog = self::dialog("D:/sf2/G");
	assert_eq!(
		click_at(&mut dialog, 0, 0),
		InputOutcome::Cancelled,
		"a click outside the dialog closes it"
	);
	let mut dialog = self::dialog("D:/sf2/G");
	assert_eq!(
		click(&mut dialog, "↑↓ — выбор"),
		InputOutcome::Pending,
		"a click on plain text inside does nothing"
	);
}

#[test]
fn the_mouse_picks_rows_and_presses_buttons() {
	let mut dialog = dialog("D:/sf2/G");
	assert_eq!(click(&mut dialog, "D:/sf2/Grand.SF2"), InputOutcome::Pending);
	assert_eq!((dialog.highlighted(), dialog.text()), (Some(1), "D:/sf2/Grand.SF2"));
	assert_eq!(click(&mut dialog, "[OK]"), InputOutcome::Submit(PathBuf::from("D:/sf2/Grand.SF2")));
	let mut dialog = self::dialog("D:/sf2/G");
	assert_eq!(click(&mut dialog, "[Отмена]"), InputOutcome::Cancelled);
}

#[test]
fn a_rejected_path_shows_the_error_until_the_field_changes() {
	let mut dialog = dialog("D:/sf2/missing.sf2");
	dialog.set_error("file not found".into());
	assert_eq!(dialog.error(), Some("file not found"));
	let (_, buf) = drawn(&mut dialog, "ru");
	let (row, _) = find(&buf, "│file not found");
	assert_eq!(row, 2, "the error sits under the field");
	assert_eq!(colours_of(&buf, "file not found").0, ERROR);

	type_text(&mut dialog, "x");
	assert_eq!(dialog.error(), None, "editing clears the error");

	dialog.set_error("still wrong".into());
	press(&mut dialog, NamedKey::Backspace);
	assert_eq!(dialog.error(), None);
	// Picking a row clears it as well.
	let mut picked = self::dialog("");
	picked.set_error("wrong".into());
	press(&mut picked, NamedKey::ArrowDown);
	assert_eq!(picked.error(), None);
}

#[test]
fn a_huge_directory_is_cut_to_the_first_matches() {
	let crowded = |_: &Path| {
		(0..1_000)
			.map(|number| DirEntry {
				name: format!("bank{number:04}.sf2"),
				is_dir: false,
			})
			.collect::<Vec<_>>()
	};
	let dialog = InputDialog::new(InputKind::Sf2File, "b", &[], crowded);
	assert_eq!(dialog.rows().len(), MAX_SUGGESTIONS);
	assert_eq!(dialog.rows()[0], "bank0000.sf2");
}
