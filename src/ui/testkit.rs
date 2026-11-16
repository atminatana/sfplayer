//! Helpers shared by the tests of the windows built on widget controls: reading a drawn layer back as
//! text, and making the mouse messages the routing entry points take.

use gterm::buffer::ScreenBuffer;
use gterm::cell::{CellFlags, Color};
use gterm_widgets::{MouseKind, MouseMsg, Rect};
use winit::event::MouseButton;
use winit::keyboard::ModifiersState;

use super::main_screen::{MIN_COLS, MIN_ROWS};

/// The smallest grid the program draws on (spec v1.2 §3).
pub const SCREEN: Rect = Rect::new(0, 0, MIN_COLS, MIN_ROWS);
/// What an untouched cell of a `ScreenBuffer` holds.
const EMPTY_CELL: char = char::MIN;

pub fn lines_of(buf: &ScreenBuffer) -> Vec<String> {
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

/// Row and column of the first character of `text` in the buffer, if it is there.
fn position(buf: &ScreenBuffer, text: &str) -> Option<(usize, usize)> {
	lines_of(buf)
		.iter()
		.enumerate()
		.find_map(|(row, line)| line.find(text).map(|byte| (row, line[..byte].chars().count())))
}

/// Row and column of the first character of `text` in the buffer.
pub fn find(buf: &ScreenBuffer, text: &str) -> (usize, usize) {
	position(buf, text).unwrap_or_else(|| panic!("{text:?} is not drawn:\n{}", lines_of(buf).join("\n")))
}

pub fn is_shown(buf: &ScreenBuffer, text: &str) -> bool {
	position(buf, text).is_some()
}

/// Foreground and background of the first cell of `text`.
pub fn colours_of(buf: &ScreenBuffer, text: &str) -> (Color, Color) {
	let (row, col) = find(buf, text);
	(buf.cell(col, row).fg, buf.cell(col, row).bg)
}

/// Whether `text` is drawn underlined — the mark of the control under the keyboard focus.
pub fn is_underlined(buf: &ScreenBuffer, text: &str) -> bool {
	let (row, col) = find(buf, text);
	buf.cell(col, row).flags.contains(CellFlags::UNDERLINE)
}

/// The screen cell of the first character of `text` in a layer drawn at `area`.
pub fn cell_of(area: Rect, buf: &ScreenBuffer, text: &str) -> (i32, i32) {
	let (row, col) = find(buf, text);
	(
		i32::from(area.x) + i32::try_from(col).expect("column"),
		i32::from(area.y) + i32::try_from(row).expect("row"),
	)
}

/// The press and the release of a left click on a screen cell.
pub fn left_click(col: i32, row: i32) -> [MouseMsg; 2] {
	[MouseKind::Down, MouseKind::Up].map(|kind| MouseMsg {
		kind,
		button: Some(MouseButton::Left),
		col,
		row,
		modifiers: ModifiersState::empty(),
	})
}
