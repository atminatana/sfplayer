//! Playing notes from the two bottom rows of the terminal keyboard (spec F-16, §6).
//!
//! Pure state machine: keys in, `MidiEvent`s out. Two note-off strategies: `Release` when the terminal
//! reports key releases (kitty keyboard protocol or the Windows console), otherwise `Gate` — a note lasts
//! `gate` or until the same key is pressed again. Key auto-repeat never retriggers a note.

use std::time::{Duration, Instant};

use crate::config::KeyboardConfig;
use crate::midi::MidiEvent;

/// Semitone offsets of the white keys from the base C: C D E F G A B C D E.
const WHITE_OFFSETS: [u8; 10] = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16];
/// Semitone offsets of the black keys: C# D# F# G# A# C# D#.
const BLACK_OFFSETS: [u8; 7] = [1, 3, 6, 8, 10, 13, 15];
pub const MIN_OCTAVE: u8 = 1;
pub const MAX_OCTAVE: u8 = 7;
pub const VELOCITY_STEP: i16 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteOffMode {
	/// The terminal reports key releases: note off on release.
	Release,
	/// No release events: a note lasts this long or until the key is pressed again.
	Gate(Duration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeldNote {
	key_char: char,
	channel: u8,
	note: u8,
	/// Gate mode only.
	off_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextKeyboard {
	white: Vec<char>,
	black: Vec<char>,
	octave: u8,
	velocity: u8,
	mode: NoteOffMode,
	held: Vec<HeldNote>,
}

impl TextKeyboard {
	pub fn new(config: &KeyboardConfig, mode: NoteOffMode) -> Self {
		Self {
			white: config.white.chars().collect(),
			black: config.black.chars().collect(),
			octave: config.base_octave.clamp(MIN_OCTAVE, MAX_OCTAVE),
			velocity: config.velocity.clamp(1, 127),
			mode,
			held: Vec::with_capacity(16),
		}
	}

	pub fn mode(&self) -> NoteOffMode {
		self.mode
	}

	pub fn octave(&self) -> u8 {
		self.octave
	}

	pub fn velocity(&self) -> u8 {
		self.velocity
	}

	pub fn held_count(&self) -> usize {
		self.held.len()
	}

	/// MIDI key for a layout character (case-insensitive), or `None` when the key is not a note.
	pub fn note_for(&self, key_char: char) -> Option<u8> {
		let lower = key_char.to_ascii_lowercase();
		let offset = if let Some(index) = self.white.iter().position(|c| *c == lower) {
			*WHITE_OFFSETS.get(index)?
		} else {
			let index = self.black.iter().position(|c| *c == lower)?;
			*BLACK_OFFSETS.get(index)?
		};
		let note = u16::from(self.octave + 1) * 12 + u16::from(offset);
		u8::try_from(note).ok().filter(|note| *note <= 127)
	}

	/// Key pressed (not auto-repeated). Returns the events to send.
	pub fn press(&mut self, key_char: char, channel: u8, now: Instant) -> Vec<MidiEvent> {
		let Some(note) = self.note_for(key_char) else {
			return Vec::new();
		};
		let lower = key_char.to_ascii_lowercase();
		if let Some(position) = self.held.iter().position(|held| held.key_char == lower) {
			return match self.mode {
				// Still held: the terminal is auto-repeating.
				NoteOffMode::Release => Vec::new(),
				// Second press ends the gated note early.
				NoteOffMode::Gate(_) => {
					let held = self.held.remove(position);
					vec![MidiEvent::NoteOff {
						channel: held.channel,
						key: held.note,
					}]
				}
			};
		}
		let off_at = match self.mode {
			NoteOffMode::Release => None,
			NoteOffMode::Gate(gate) => Some(now + gate),
		};
		self.held.push(HeldNote {
			key_char: lower,
			channel,
			note,
			off_at,
		});
		vec![MidiEvent::NoteOn {
			channel,
			key: note,
			velocity: self.velocity,
		}]
	}

	/// Key released (release mode only; ignored in gate mode).
	pub fn release(&mut self, key_char: char) -> Vec<MidiEvent> {
		if self.mode != NoteOffMode::Release {
			return Vec::new();
		}
		let lower = key_char.to_ascii_lowercase();
		match self.held.iter().position(|held| held.key_char == lower) {
			Some(position) => {
				let held = self.held.remove(position);
				vec![MidiEvent::NoteOff {
					channel: held.channel,
					key: held.note,
				}]
			}
			None => Vec::new(),
		}
	}

	/// Gate timer: note offs whose time has come.
	pub fn tick(&mut self, now: Instant) -> Vec<MidiEvent> {
		let mut events = Vec::new();
		self.held.retain(|held| match held.off_at {
			Some(off_at) if now >= off_at => {
				events.push(MidiEvent::NoteOff {
					channel: held.channel,
					key: held.note,
				});
				false
			}
			_ => true,
		});
		events
	}

	/// Esc: note off for everything the text keyboard holds.
	pub fn all_notes_off(&mut self) -> Vec<MidiEvent> {
		self.held
			.drain(..)
			.map(|held| MidiEvent::NoteOff {
				channel: held.channel,
				key: held.note,
			})
			.collect()
	}

	/// Forget held notes without sending anything (after a panic, which already silenced the engine).
	pub fn clear(&mut self) {
		self.held.clear();
	}

	pub fn octave_up(&mut self) {
		self.octave = (self.octave + 1).min(MAX_OCTAVE);
	}

	pub fn octave_down(&mut self) {
		self.octave = self.octave.saturating_sub(1).max(MIN_OCTAVE);
	}

	pub fn velocity_up(&mut self) {
		self.velocity = (i16::from(self.velocity) + VELOCITY_STEP).min(127) as u8;
	}

	pub fn velocity_down(&mut self) {
		self.velocity = (i16::from(self.velocity) - VELOCITY_STEP).max(1) as u8;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const GATE: Duration = Duration::from_millis(300);

	fn release_keyboard() -> TextKeyboard {
		TextKeyboard::new(&KeyboardConfig::default(), NoteOffMode::Release)
	}

	fn gate_keyboard() -> TextKeyboard {
		TextKeyboard::new(&KeyboardConfig::default(), NoteOffMode::Gate(GATE))
	}

	#[test]
	fn default_layout_maps_two_rows_to_c4_through_e5() {
		let keyboard = release_keyboard();
		let whites: Vec<u8> = "zxcvbnm,./".chars().map(|c| keyboard.note_for(c).unwrap()).collect();
		assert_eq!(whites, vec![60, 62, 64, 65, 67, 69, 71, 72, 74, 76]);
		let blacks: Vec<u8> = "sdghjl;".chars().map(|c| keyboard.note_for(c).unwrap()).collect();
		assert_eq!(blacks, vec![61, 63, 66, 68, 70, 73, 75]);
		assert_eq!(keyboard.note_for('Z'), Some(60), "case-insensitive");
		assert_eq!(keyboard.note_for('q'), None);
		assert_eq!(keyboard.note_for('1'), None);
	}

	#[test]
	fn octave_and_velocity_are_clamped() {
		let mut keyboard = release_keyboard();
		for _ in 0..10 {
			keyboard.octave_up();
		}
		assert_eq!(keyboard.octave(), MAX_OCTAVE);
		assert_eq!(keyboard.note_for('/'), Some(112), "E8 at octave 7 still fits");
		for _ in 0..10 {
			keyboard.octave_down();
		}
		assert_eq!(keyboard.octave(), MIN_OCTAVE);
		assert_eq!(keyboard.note_for('z'), Some(24));
		for _ in 0..20 {
			keyboard.velocity_up();
		}
		assert_eq!(keyboard.velocity(), 127);
		for _ in 0..20 {
			keyboard.velocity_down();
		}
		assert_eq!(keyboard.velocity(), 1);
	}

	#[test]
	fn release_mode_sends_note_off_on_release_and_ignores_autorepeat() {
		let mut keyboard = release_keyboard();
		let now = Instant::now();
		assert_eq!(
			keyboard.press('z', 0, now),
			vec![MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			}]
		);
		assert!(keyboard.press('z', 0, now).is_empty(), "auto-repeat");
		assert!(keyboard.press('Z', 0, now).is_empty(), "shifted repeat of the same key");
		assert!(keyboard.tick(now + Duration::from_secs(10)).is_empty(), "no gate in release mode");
		assert_eq!(keyboard.release('z'), vec![MidiEvent::NoteOff { channel: 0, key: 60 }]);
		assert!(keyboard.release('z').is_empty());
	}

	#[test]
	fn gate_mode_times_out_or_ends_on_second_press() {
		let mut keyboard = gate_keyboard();
		let now = Instant::now();
		assert_eq!(
			keyboard.press('x', 3, now),
			vec![MidiEvent::NoteOn {
				channel: 3,
				key: 62,
				velocity: 100
			}]
		);
		assert!(keyboard.release('x').is_empty(), "releases are ignored in gate mode");
		assert!(keyboard.tick(now + Duration::from_millis(299)).is_empty());
		assert_eq!(keyboard.tick(now + GATE), vec![MidiEvent::NoteOff { channel: 3, key: 62 }]);
		assert_eq!(keyboard.held_count(), 0);
		keyboard.press('x', 3, now);
		assert_eq!(
			keyboard.press('x', 3, now + Duration::from_millis(50)),
			vec![MidiEvent::NoteOff { channel: 3, key: 62 }],
			"second press ends the note"
		);
	}

	#[test]
	fn all_notes_off_and_clear() {
		let mut keyboard = release_keyboard();
		let now = Instant::now();
		keyboard.press('z', 0, now);
		keyboard.press('s', 0, now);
		let mut off = keyboard.all_notes_off();
		off.sort_by_key(|event| match event {
			MidiEvent::NoteOff { key, .. } => *key,
			_ => 0,
		});
		assert_eq!(
			off,
			vec![
				MidiEvent::NoteOff { channel: 0, key: 60 },
				MidiEvent::NoteOff { channel: 0, key: 61 }
			]
		);
		keyboard.press('z', 0, now);
		keyboard.clear();
		assert_eq!(keyboard.held_count(), 0);
		assert!(keyboard.release('z').is_empty());
	}

	#[test]
	fn custom_layout_from_config() {
		let config = KeyboardConfig {
			white: "qwertyuiop".into(),
			black: "23567890".into(),
			base_octave: 3,
			velocity: 64,
			..KeyboardConfig::default()
		};
		let keyboard = TextKeyboard::new(&config, NoteOffMode::Release);
		assert_eq!(keyboard.note_for('q'), Some(48));
		assert_eq!(keyboard.note_for('2'), Some(49));
		assert_eq!(keyboard.note_for('z'), None);
		assert_eq!(keyboard.velocity(), 64);
	}
}
