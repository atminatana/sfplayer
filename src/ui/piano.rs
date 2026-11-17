//! The on-screen piano (spec v1.2, F-24): 41×4 cells showing the range of the text keyboard — ten white
//! keys from the base C, seven black ones — each with the character that plays it, the sounding notes
//! lit, and playable with the mouse.
//!
//! ```text
//! │  │s│ │d│  │  │g│ │h│ │j│  │  │l│ │;│  │      the black keys
//! │  └┬┘ └┬┘  │  └┬┘ └┬┘ └┬┘  │  └┬┘ └┬┘  │
//! │ z │ x │ c │ v │ b │ n │ m │ , │ . │ / │      the white keys, four cells each
//! └───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘
//! ```
//!
//! A press plays the key under the pointer and a release stops it; dragging over the keys plays each in
//! turn, and leaving the piano with the button down stops the note until the pointer comes back.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::{MouseKind, MouseMsg, Style};
use winit::event::MouseButton;

use super::paint::{BACKGROUND, HINT, LABEL, Pen, VALUE};
use super::text_keyboard::{BLACK_OFFSETS, WHITE_OFFSETS};
use crate::midi::MidiEvent;

/// Cells a white key takes, its left border included.
const WHITE_CELLS: u16 = 4;
const WHITE_KEYS: u16 = 10;
pub const WIDTH: u16 = WHITE_KEYS * WHITE_CELLS + 1;
pub const HEIGHT: u16 = 4;
/// The row of the white keys' characters; the rows above it are shared with the black keys, the row
/// under it is the line that closes the keys.
const WHITE_ROW: u16 = 2;
const BOTTOM_ROW: u16 = HEIGHT - 1;
/// Cells a black key takes: its character between two borders.
const BLACK_CELLS: u16 = 3;
const BORDER: &str = "│";
/// What closes a black key underneath: its middle cell runs on as the border of the white keys.
const BLACK_FOOT: &str = "└┬┘";
/// The line under the white keys: its corners, the cell under a border between two keys, the rest.
const BOTTOM_LEFT: &str = "└";
const BOTTOM_RIGHT: &str = "┘";
const BOTTOM_JOINT: &str = "┴";
const BOTTOM_LINE: &str = "─";
/// White keys that have a black key to their right, in the order of `BLACK_OFFSETS`.
const WHITE_BEFORE_BLACK: [u16; 7] = [0, 1, 3, 4, 5, 7, 8];

/// First of the three cells of black key `index`: the key sits over the border to the next white key.
fn black_column(index: usize) -> u16 {
	(WHITE_BEFORE_BLACK[index] + 1) * WHITE_CELLS - 1
}

/// First of the three cells of white key `index` in the bottom row, after its left border.
fn white_column(index: usize) -> u16 {
	u16::try_from(index)
		.unwrap_or(u16::MAX)
		.saturating_mul(WHITE_CELLS)
		.saturating_add(1)
}

/// Semitones from the base C of the key drawn at cell (`column`, `row`) of the piano; `None` outside it.
/// The two upper rows belong to the black keys where they are and to the white key underneath
/// elsewhere; the row of the characters and the line under it belong to the white keys.
pub fn key_at(column: u16, row: u16) -> Option<u8> {
	if column >= WIDTH || row >= HEIGHT {
		return None;
	}
	if row < WHITE_ROW {
		let black = (0..BLACK_OFFSETS.len()).find(|index| (black_column(*index)..black_column(*index) + BLACK_CELLS).contains(&column));
		if let Some(index) = black {
			return Some(BLACK_OFFSETS[index]);
		}
	}
	let white = usize::from(column / WHITE_CELLS).min(WHITE_OFFSETS.len() - 1);
	Some(WHITE_OFFSETS[white])
}

/// The characters written on the keys: those that play them on the text keyboard, lowest key first.
#[derive(Debug, Clone, Copy)]
pub struct KeyLabels<'a> {
	pub white: &'a [char],
	pub black: &'a [char],
}

/// The character of key `index`; a key the layout gives no character stays blank.
fn label(labels: &[char], index: usize) -> char {
	labels.get(index).copied().unwrap_or(' ')
}

/// How the character of a key is written: inverted on amber while its note sounds.
fn label_style(is_lit: bool) -> Style {
	if is_lit {
		Style::new().fg(BACKGROUND).bg(VALUE)
	} else {
		Style::new().fg(LABEL)
	}
}

/// Draws the piano with its top-left cell at (`x`, `y`). `base_key` is the MIDI key of its first C;
/// `sounding` has a bit per MIDI key, and the keys of the piano that are set in it are lit.
pub fn draw(buf: &mut ScreenBuffer, x: u16, y: u16, base_key: u8, labels: KeyLabels<'_>, sounding: u128) {
	let is_lit = |offset: u8| {
		let key = u32::from(base_key) + u32::from(offset);
		key < 128 && sounding & (1u128 << key) != 0
	};
	// The borders of the white keys run down to the line that closes them; a black key is drawn over
	// the upper part of the border it sits on.
	for border in 0..=WHITE_KEYS {
		for row in 0..BOTTOM_ROW {
			Pen::at(buf, x + border * WHITE_CELLS, y + row).text(BORDER, HINT);
		}
	}
	for column in 0..WIDTH {
		let glyph = match column {
			0 => BOTTOM_LEFT,
			column if column == WIDTH - 1 => BOTTOM_RIGHT,
			column if column % WHITE_CELLS == 0 => BOTTOM_JOINT,
			_ => BOTTOM_LINE,
		};
		Pen::at(buf, x + column, y + BOTTOM_ROW).text(glyph, HINT);
	}
	for (index, offset) in BLACK_OFFSETS.iter().enumerate() {
		let column = x + black_column(index);
		Pen::at(buf, column, y)
			.text(BORDER, HINT)
			.styled(&label(labels.black, index).to_string(), label_style(is_lit(*offset)))
			.text(BORDER, HINT);
		Pen::at(buf, column, y + 1).text(BLACK_FOOT, HINT);
	}
	for (index, offset) in WHITE_OFFSETS.iter().enumerate() {
		let cap = format!(" {} ", label(labels.white, index));
		Pen::at(buf, x + white_column(index), y + WHITE_ROW).styled(&cap, label_style(is_lit(*offset)));
	}
}

/// What the mouse is doing on the piano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
	/// The left button is not down, or went down somewhere else.
	Idle,
	/// The button went down on the piano and is still down; the MIDI key it holds now, if the pointer
	/// is over one.
	Held(Option<u8>),
}

/// The mouse on the piano: which note it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piano {
	channel: u8,
	drag: Drag,
}

impl Piano {
	/// A piano whose notes go to MIDI channel `channel` (0-based).
	pub fn new(channel: u8) -> Self {
		Self { channel, drag: Drag::Idle }
	}

	/// Whether the button that went down on the piano is still down: until it comes up, the mouse
	/// belongs to the piano wherever the pointer is.
	pub fn is_dragging(&self) -> bool {
		self.drag != Drag::Idle
	}

	/// A mouse event in cells of the grid. `origin` is the top-left cell of the piano, `None` while it
	/// is not on the screen. Returns the notes to send, or `None` when the event is not for the piano.
	pub fn handle_mouse(&mut self, message: &MouseMsg, origin: Option<(u16, u16)>, base_key: u8, velocity: u8) -> Option<Vec<MidiEvent>> {
		let under_pointer = origin.and_then(|origin| key_under(message, origin, base_key));
		match (self.drag, message.kind) {
			(Drag::Idle, MouseKind::Down) if message.is_left_down() => {
				let key = under_pointer?;
				self.drag = Drag::Held(Some(key));
				Some(vec![self.note_on(key, velocity)])
			}
			(Drag::Idle, _) => None,
			(Drag::Held(held), MouseKind::Move) => {
				if under_pointer == held {
					return Some(Vec::new());
				}
				self.drag = Drag::Held(under_pointer);
				let stopped = held.map(|key| self.note_off(key));
				let started = under_pointer.map(|key| self.note_on(key, velocity));
				Some(stopped.into_iter().chain(started).collect())
			}
			(Drag::Held(_), MouseKind::Up) if message.button == Some(MouseButton::Left) => Some(self.release()),
			(Drag::Held(_), MouseKind::Cancel) => Some(self.release()),
			// Another button or the wheel while the left one is down: still the piano's, and nothing to play.
			(Drag::Held(_), _) => Some(Vec::new()),
		}
	}

	/// Lets go of the note the mouse holds: the button came up, the pointer left the window, or the
	/// window lost the focus.
	pub fn release(&mut self) -> Vec<MidiEvent> {
		let held = std::mem::replace(&mut self.drag, Drag::Idle);
		match held {
			Drag::Held(Some(key)) => vec![self.note_off(key)],
			Drag::Held(None) | Drag::Idle => Vec::new(),
		}
	}

	fn note_on(&self, key: u8, velocity: u8) -> MidiEvent {
		MidiEvent::NoteOn {
			channel: self.channel,
			key,
			velocity,
		}
	}

	fn note_off(&self, key: u8) -> MidiEvent {
		MidiEvent::NoteOff {
			channel: self.channel,
			key,
		}
	}
}

/// The MIDI key of the piano under the pointer, if it is over the piano and the key exists.
fn key_under(message: &MouseMsg, (x, y): (u16, u16), base_key: u8) -> Option<u8> {
	let column = u16::try_from(message.col.checked_sub(i32::from(x))?).ok()?;
	let row = u16::try_from(message.row.checked_sub(i32::from(y))?).ok()?;
	let key = base_key.checked_add(key_at(column, row)?)?;
	(key < 128).then_some(key)
}

#[cfg(test)]
mod tests;
