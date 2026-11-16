//! Tests of the help window: keys and the mouse go through the routing entry points the window uses,
//! the picture is read back from the modal layer's buffer. No window.

use gterm::buffer::ScreenBuffer;
use gterm::cell::CellFlags;
use gterm_widgets::{KeyMsg, MouseMsg, Rect};
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

use super::*;
use crate::ui::keys::Bindings;
use crate::ui::testkit::{SCREEN, cell_of, colours_of, find, is_shown, is_underlined, left_click, lines_of, wheel};

/// A grid with room for the whole table of the default bindings.
const TALL_SCREEN: Rect = Rect::new(0, 0, SCREEN.width, 60);

fn default_rows() -> Vec<(String, String)> {
	Bindings::default().help_rows("ru")
}

fn dialog() -> HelpDialog {
	HelpDialog::new(default_rows())
}

fn press_key(dialog: &mut HelpDialog, key: &Key) -> HelpOutcome {
	let message = KeyMsg {
		key,
		text: None,
		modifiers: ModifiersState::empty(),
	};
	let routed = dialog.ui("ru", SCREEN.height).route_key(&message);
	dialog.settle(routed, Some(key))
}

fn press(dialog: &mut HelpDialog, named: NamedKey) -> HelpOutcome {
	press_key(dialog, &Key::Named(named))
}

/// The window as drawn on a grid: where it sits and its cells.
fn drawn_on(dialog: &mut HelpDialog, locale: &str, screen: Rect) -> (Rect, ScreenBuffer) {
	dialog
		.ui(locale, screen.height)
		.modal_buffer(screen)
		.expect("the window is a modal layer")
}

fn drawn(dialog: &mut HelpDialog) -> (Rect, ScreenBuffer) {
	drawn_on(dialog, "ru", SCREEN)
}

/// Mouse messages routed and settled one after another; the answer of whichever gave one.
fn send(dialog: &mut HelpDialog, messages: &[MouseMsg]) -> HelpOutcome {
	let mut outcome = HelpOutcome::Pending;
	for message in messages {
		let routed = dialog.ui("ru", SCREEN.height).route_mouse(message, SCREEN);
		let step = dialog.settle(routed, None);
		if step != HelpOutcome::Pending {
			outcome = step;
		}
	}
	outcome
}

fn click(dialog: &mut HelpDialog, text: &str) -> HelpOutcome {
	let (area, buf) = drawn(dialog);
	let (col, row) = cell_of(area, &buf, text);
	send(dialog, &left_click(col, row))
}

/// The rows of the table as drawn, top to bottom: the keys and what they do, as the window words them.
fn table_of(buf: &ScreenBuffer) -> Vec<(String, String)> {
	let lines = lines_of(buf);
	// Between the top of the frame and the row of the button; the bar, if any, is the last cell.
	lines[1..lines.len() - 2]
		.iter()
		.map(|line| {
			let inside: String = line.chars().skip(1).take(usize::from(WIDTH) - 3).collect();
			let (keys, action) = inside.split_once("  ").expect("two columns");
			(keys.trim_end().to_owned(), action.trim().to_owned())
		})
		.collect()
}

/// The last cell inside the frame in the first row of the table: where the scrollbar stands.
fn bar_cell(buf: &ScreenBuffer) -> char {
	match buf.cell(usize::from(WIDTH) - 2, 1).ch {
		char::MIN => ' ',
		ch => ch,
	}
}

#[test]
fn the_window_shows_the_table_of_the_bindings_in_force() {
	let rows = default_rows();
	let mut dialog = dialog();
	let (area, buf) = drawn(&mut dialog);
	assert_eq!(area.width, WIDTH);
	assert_eq!(area.height, SCREEN.height - 4, "two rows of the grid stay free above and below");
	assert_eq!((area.x, area.y), ((SCREEN.width - WIDTH) / 2, 2), "the window is centred");
	let lines = lines_of(&buf);
	assert!(lines[0].starts_with("┌ Справка ─"), "{}", lines[0]);
	assert!(lines[usize::from(area.height) - 1].starts_with("└─"));

	// What is drawn is the table of the bindings, row for row, as far as the window goes.
	let shown = table_of(&buf);
	assert_eq!(shown.len(), usize::from(area.height - CHROME_ROWS));
	assert!(shown.len() < rows.len(), "on the smallest grid the table does not fit");
	assert_eq!(shown, rows[..shown.len()]);
	assert_ne!(bar_cell(&buf), ' ', "a table that does not fit has a scrollbar");

	let (first_row, first_col) = find(&buf, &rows[0].0);
	assert!(buf.cell(first_col, first_row).flags.contains(CellFlags::BOLD), "the keys are bold");
	assert_eq!(colours_of(&buf, &rows[0].0), (BACKGROUND, TEXT), "the first row is highlighted");
	assert_eq!(colours_of(&buf, &rows[1].0), (TEXT, BACKGROUND));
	assert_eq!(dialog.highlighted(), Some(0));
	assert!(is_shown(&buf, "│↑↓ PgUp PgDn — прокрутка · Esc или F1 — закрыть"));
	assert!(is_shown(&buf, "[Закрыть]│"));
	assert_eq!(colours_of(&buf, "↑↓ PgUp").0, HINT);

	let mut english = HelpDialog::new(Bindings::default().help_rows("en"));
	let (_, buf) = drawn_on(&mut english, "en", SCREEN);
	assert!(lines_of(&buf)[0].starts_with("┌ Help ─"));
	assert!(is_shown(&buf, "[Close]│"));
}

#[test]
fn a_table_that_fits_is_shown_whole_and_has_no_scrollbar() {
	let rows = default_rows();
	let mut dialog = dialog();
	let (area, buf) = drawn_on(&mut dialog, "ru", TALL_SCREEN);
	let wanted = u16::try_from(rows.len()).expect("a short table") + CHROME_ROWS;
	assert_eq!(area.height, wanted, "the window is as high as its table");
	assert_eq!(table_of(&buf), rows);
	assert_eq!(bar_cell(&buf), ' ');

	// A table of other bindings is shown as it is given.
	let few = vec![
		("F1".to_owned(), "Эта справка".to_owned()),
		("Ctrl+Q / F10".to_owned(), "Выход".to_owned()),
	];
	let mut short = HelpDialog::new(few.clone());
	let (area, buf) = drawn(&mut short);
	assert_eq!(area.height, 2 + CHROME_ROWS);
	assert_eq!(table_of(&buf), few);
	let (_, action_col) = find(&buf, "Эта справка");
	let (_, other_col) = find(&buf, "Выход");
	assert_eq!(action_col, other_col, "the second column starts after the widest keys");

	let mut empty = HelpDialog::new(Vec::new());
	let (area, _) = drawn(&mut empty);
	assert_eq!(area.height, 1 + CHROME_ROWS);
	assert_eq!(empty.highlighted(), None);
}

#[test]
fn the_arrows_and_the_page_keys_move_through_the_table() {
	let rows = default_rows();
	let last = rows.len() - 1;
	let mut dialog = dialog();
	drawn(&mut dialog);
	assert_eq!(press(&mut dialog, NamedKey::ArrowDown), HelpOutcome::Pending);
	assert_eq!(dialog.highlighted(), Some(1));
	let (_, buf) = drawn(&mut dialog);
	assert_eq!(colours_of(&buf, &rows[1].0), (BACKGROUND, TEXT));
	press(&mut dialog, NamedKey::ArrowUp);
	press(&mut dialog, NamedKey::ArrowUp);
	assert_eq!(dialog.highlighted(), Some(0), "the highlight stops at the top");

	// End goes to the last row, and the table scrolls to show it.
	press(&mut dialog, NamedKey::End);
	let (_, buf) = drawn(&mut dialog);
	assert_eq!(dialog.highlighted(), Some(last));
	let shown = table_of(&buf);
	assert_eq!(shown.last(), Some(&rows[last]));
	assert_ne!(shown[0], rows[0], "the first rows are scrolled away");
	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(dialog.highlighted(), Some(last), "the highlight stops at the bottom");

	press(&mut dialog, NamedKey::Home);
	let (_, buf) = drawn(&mut dialog);
	assert_eq!(dialog.highlighted(), Some(0));
	assert_eq!(table_of(&buf)[0], rows[0]);

	// A page is what the table shows at once.
	press(&mut dialog, NamedKey::PageDown);
	let page = dialog.highlighted().expect("a row is highlighted");
	assert!(
		page > 1 && page < last,
		"a page down is more than a row and less than the table: {page}"
	);
	press(&mut dialog, NamedKey::PageUp);
	assert_eq!(dialog.highlighted(), Some(0));
}

#[test]
fn the_wheel_scrolls_the_table_and_leaves_the_highlight_where_it_is() {
	let rows = default_rows();
	let mut dialog = dialog();
	let (area, buf) = drawn(&mut dialog);
	let (col, row) = cell_of(area, &buf, &rows[3].1);
	assert_eq!(send(&mut dialog, &[wheel(col, row, -3.0)]), HelpOutcome::Pending);
	let (_, buf) = drawn(&mut dialog);
	assert_eq!(table_of(&buf)[0], rows[3], "three rows down");
	assert_eq!(dialog.highlighted(), Some(0));
	send(&mut dialog, &[wheel(col, row, 1.0)]);
	let (_, buf) = drawn(&mut dialog);
	assert_eq!(table_of(&buf)[0], rows[2], "one row back up");
	// Past the end the table stops at its last page.
	send(&mut dialog, &[wheel(col, row, -500.0)]);
	let (_, buf) = drawn(&mut dialog);
	assert_eq!(table_of(&buf).last(), Some(&rows[rows.len() - 1]));
}

#[test]
fn esc_enter_f1_the_button_and_a_click_outside_close_the_window() {
	assert_eq!(press(&mut dialog(), NamedKey::Escape), HelpOutcome::Closed);
	assert_eq!(press(&mut dialog(), NamedKey::Enter), HelpOutcome::Closed);
	assert_eq!(press(&mut dialog(), NamedKey::F1), HelpOutcome::Closed);
	assert_eq!(click(&mut dialog(), "[Закрыть]"), HelpOutcome::Closed);
	assert_eq!(send(&mut dialog(), &left_click(0, 0)), HelpOutcome::Closed);

	// With the keyboard: Tab goes to the button, Enter or Space press it.
	for key in [NamedKey::Enter, NamedKey::Space] {
		let mut dialog = dialog();
		assert_eq!(press(&mut dialog, NamedKey::Tab), HelpOutcome::Pending);
		let (_, buf) = drawn(&mut dialog);
		assert!(is_underlined(&buf, "[Закрыть]"));
		assert_eq!(press(&mut dialog, key), HelpOutcome::Closed, "{key:?}");
	}
}

#[test]
fn nothing_else_closes_the_window() {
	let rows = default_rows();
	let mut dialog = dialog();
	for key in [
		NamedKey::F3,
		NamedKey::F10,
		NamedKey::Space,
		NamedKey::Backspace,
		NamedKey::ArrowLeft,
	] {
		assert_eq!(press(&mut dialog, key), HelpOutcome::Pending, "{key:?}");
	}
	assert_eq!(press_key(&mut dialog, &Key::Character(SmolStr::new("z"))), HelpOutcome::Pending);
	// A click on a row highlights it.
	assert_eq!(click(&mut dialog, &rows[4].1), HelpOutcome::Pending);
	assert_eq!(dialog.highlighted(), Some(4));
	// A held key is no new press: the window gets the key only once it is pressed anew.
	let key = Key::Named(NamedKey::F1);
	let message = KeyMsg {
		key: &key,
		text: None,
		modifiers: ModifiersState::empty(),
	};
	let routed = dialog.ui("ru", SCREEN.height).route_key(&message);
	assert_eq!(dialog.settle(routed, None), HelpOutcome::Pending);
}
