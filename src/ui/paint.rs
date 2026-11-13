//! Shared drawing vocabulary of the window (spec v1.2 §3): the palette of the mock-up and a pen that
//! writes consecutive pieces of one row into a `ScreenBuffer`.

use gterm::buffer::ScreenBuffer;
use gterm::cell::Color;
use gterm_widgets::{BufferExt, Rect, Style};

// Palette of the mock-up: black background, light text, grey labels and rules, amber values and active
// buttons, green for "fine", red for errors.
pub const BACKGROUND: Color = Color::from_rgb(0x00, 0x00, 0x00);
pub const TEXT: Color = Color::from_rgb(0xE5, 0xE5, 0xE5);
pub const LABEL: Color = Color::from_rgb(0x73, 0x73, 0x73);
pub const RULE: Color = Color::from_rgb(0x40, 0x40, 0x40);
pub const HINT: Color = Color::from_rgb(0x60, 0x60, 0x60);
pub const DISABLED: Color = Color::from_rgb(0x40, 0x40, 0x40);
pub const VALUE: Color = Color::from_rgb(0xFC, 0xD3, 0x4D);
pub const GOOD: Color = Color::from_rgb(0x22, 0xC5, 0x5E);
pub const ERROR: Color = Color::from_rgb(0xEF, 0x44, 0x44);
pub const MESSAGE: Color = Color::from_rgb(0xFA, 0xCC, 0x15);
/// Unlit cells of a meter: a shade glyph covers a quarter of the cell, so a rule-dark colour would vanish.
pub const METER_TRACK: Color = LABEL;

/// The highlighted row of a list, the cursor cell of a field: light background, dark text.
pub const SELECTED: Style = Style::new().fg(BACKGROUND).bg(TEXT);

/// Writes consecutive pieces of one row, advancing to the right; nothing lands outside its clip area.
pub(super) struct Pen<'a> {
	buf: &'a mut ScreenBuffer,
	clip: Rect,
	pub(super) x: u16,
	pub(super) y: u16,
}

impl<'a> Pen<'a> {
	/// A pen at `(x, y)` that may write anywhere in the buffer.
	pub(super) fn at(buf: &'a mut ScreenBuffer, x: u16, y: u16) -> Self {
		let clip = buf.area();
		Self { buf, clip, x, y }
	}

	/// A pen at column `column` and row `row` of `area`, confined to it.
	pub(super) fn within(buf: &'a mut ScreenBuffer, area: Rect, column: u16, row: u16) -> Self {
		Self {
			buf,
			clip: area,
			x: area.x.saturating_add(column),
			y: area.y.saturating_add(row),
		}
	}

	pub(super) fn text(&mut self, text: &str, color: Color) -> &mut Self {
		self.styled(text, Style::new().fg(color))
	}

	pub(super) fn styled(&mut self, text: &str, style: Style) -> &mut Self {
		self.buf.set_string(self.x, self.y, text, style, self.clip);
		let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
		self.x = self.x.saturating_add(width);
		self
	}

	/// The column just past the right edge of what the pen may write into.
	pub(super) fn clip_width(&self) -> u16 {
		self.clip.x.saturating_add(self.clip.width)
	}

	pub(super) fn gap(&mut self, cells: u16) -> &mut Self {
		self.x = self.x.saturating_add(cells);
		self
	}
}
