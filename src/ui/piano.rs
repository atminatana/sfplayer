//! The on-screen piano (spec v1.2, F-24): 31×2 cells showing the range of the text keyboard — ten white
//! keys from the base C, seven black ones — with the sounding notes lit, and playable with the mouse.
//!
//! ```text
//! │ ██ ██  │ ██ ██ ██  │ ██ ██  │      the black keys
//! │  │  │  │  │  │  │  │  │  │  │      the white keys, three cells each
//! ```
//!
//! A press plays the key under the pointer and a release stops it; dragging over the keys plays each in
//! turn, and leaving the piano with the button down stops the note until the pointer comes back.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::{MouseKind, MouseMsg};
use winit::event::MouseButton;

use super::paint::{HINT, Pen, VALUE};
use super::text_keyboard::{BLACK_OFFSETS, WHITE_OFFSETS};
use crate::midi::MidiEvent;

pub const WIDTH: u16 = 31;
pub const HEIGHT: u16 = 2;
/// Cells a white key takes, its left border included.
const WHITE_CELLS: u16 = 3;
const TOP: &str = "│ ██ ██  │ ██ ██ ██  │ ██ ██  │";
const BOTTOM: &str = "│  │  │  │  │  │  │  │  │  │  │";
const LIT: &str = "██";
/// White keys that have a black key to their right, in the order of `BLACK_OFFSETS`.
const WHITE_BEFORE_BLACK: [u16; 7] = [0, 1, 3, 4, 5, 7, 8];

/// First of the two cells of black key `index`: over the border to the next white key.
fn black_column(index: usize) -> u16 {
	WHITE_BEFORE_BLACK[index] * WHITE_CELLS + 2
}

/// First of the two cells of white key `index` in the bottom row, after its left border.
fn white_column(index: usize) -> u16 {
	u16::try_from(index)
		.unwrap_or(u16::MAX)
		.saturating_mul(WHITE_CELLS)
		.saturating_add(1)
}

/// Semitones from the base C of the key drawn at cell (`column`, `row`) of the piano; `None` outside it.
/// The top row belongs to the black keys where they are and to the white key underneath elsewhere.
pub fn key_at(column: u16, row: u16) -> Option<u8> {
	if column >= WIDTH || row >= HEIGHT {
		return None;
	}
	if row == 0 {
		let black = (0..BLACK_OFFSETS.len()).find(|index| (black_column(*index)..black_column(*index) + 2).contains(&column));
		if let Some(index) = black {
			return Some(BLACK_OFFSETS[index]);
		}
	}
	let white = usize::from(column / WHITE_CELLS).min(WHITE_OFFSETS.len() - 1);
	Some(WHITE_OFFSETS[white])
}

/// Draws the piano with its top-left cell at (`x`, `y`). `base_key` is the MIDI key of its first C;
/// `sounding` has a bit per MIDI key, and the keys of the piano that are set in it are lit.
pub fn draw(buf: &mut ScreenBuffer, x: u16, y: u16, base_key: u8, sounding: u128) {
	Pen::at(buf, x, y).text(TOP, HINT);
	Pen::at(buf, x, y + 1).text(BOTTOM, HINT);
	let is_lit = |offset: u8| {
		let key = u32::from(base_key) + u32::from(offset);
		key < 128 && sounding & (1u128 << key) != 0
	};
	for (index, offset) in BLACK_OFFSETS.iter().enumerate() {
		if is_lit(*offset) {
			Pen::at(buf, x + black_column(index), y).text(LIT, VALUE);
		}
	}
	for (index, offset) in WHITE_OFFSETS.iter().enumerate() {
		if is_lit(*offset) {
			Pen::at(buf, x + white_column(index), y + 1).text(LIT, VALUE);
		}
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
