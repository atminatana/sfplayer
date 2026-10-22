//! Playing notes from the two bottom rows of the terminal keyboard (spec F-16, §6).
//!
//! Pure state machine: keys in, `MidiEvent`s out. Two note-off strategies: `Release` when the terminal
//! reports key releases (kitty keyboard protocol or the Windows console), otherwise `Gate` — a note lasts
//! `gate` or until the same key is pressed again. Key auto-repeat never retriggers a note: terminals that
//! repeat by pairing Release+Press (X11) or Press+Release (Windows ConPTY) are debounced in release mode,
//! and gate mode ignores presses that come too soon after the previous one.

use std::collections::HashMap;
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

/// Terminals that auto-repeat by pairing Release+Press (X11, ~2 ms apart) must not cut the note:
/// a release only takes effect after this window, and a press inside it cancels the note off.
const RELEASE_DEBOUNCE: Duration = Duration::from_millis(20);
/// A release this soon after its own press is not a human one: ConPTY-hosted terminals on Windows
/// (VS Code, mintty, ...) synthesize a Press+Release pair ~3 ms apart for every character.
const SYNTHESIZED_RELEASE_GAP: Duration = Duration::from_millis(15);
/// Debounce for a synthesized release: the key is still down until the auto-repeat pairs stop, and
/// the next pair follows the previous one by the repeat period (Windows fastest rate: ~31 ms, with
/// ~47 ms hiccups), so the window must outlast that gap. Real releases keep `RELEASE_DEBOUNCE`.
const PAIRED_RELEASE_DEBOUNCE: Duration = Duration::from_millis(100);
/// Gate mode: a same-key press closer than this to the previous press is auto-repeat (X11 repeats
/// every ~30–40 ms). The anchor moves on every seen press, so a sustained stream stays swallowed;
/// the first repeat after the initial delay still plays — with no release events it is
/// indistinguishable from a deliberate re-press.
const GATE_REPEAT_GUARD: Duration = Duration::from_millis(100);

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
	/// When the note goes off by itself: the gate deadline, or in release mode the end of the debounce
	/// window after the key went up (`None` while the key is down).
	off_at: Option<Instant>,
	/// The last seen press, to tell a synthesized release from a real one.
	pressed_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextKeyboard {
	white: Vec<char>,
	black: Vec<char>,
	octave: u8,
	velocity: u8,
	mode: NoteOffMode,
	held: Vec<HeldNote>,
	/// Gate mode only: the last seen press of each note key, for the repeat guard.
	last_press: HashMap<char, Instant>,
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
			last_press: HashMap::new(),
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

	/// Key pressed; auto-repeat is swallowed (by the held check in release mode, by the cadence
	/// guard in gate mode). Returns the events to send.
	pub fn press(&mut self, key_char: char, channel: u8, now: Instant) -> Vec<MidiEvent> {
		let Some(note) = self.note_for(key_char) else {
			return Vec::new();
		};
		let lower = key_char.to_ascii_lowercase();
		if matches!(self.mode, NoteOffMode::Gate(_)) {
			// The anchor moves on every seen press — swallowed ones included — so the ~37 ms
			// repeat stream never gets further than the guard from its own previous press.
			let fresh = self
				.last_press
				.get(&lower)
				.is_none_or(|previous| now.saturating_duration_since(*previous) >= GATE_REPEAT_GUARD);
			self.last_press.insert(lower, now);
			if !fresh {
				return Vec::new();
			}
		}
		let mut events = self.flush_expired(now);
		if let Some(position) = self.held.iter().position(|held| held.key_char == lower) {
			self.held[position].pressed_at = now;
			return match self.mode {
				// The terminal is auto-repeating: either the key never went up, or flush_expired
				// kept the note because its release is still inside the debounce window - that
				// release was part of a repeat pair, so the note off is cancelled.
				NoteOffMode::Release => {
					self.held[position].off_at = None;
					events
				}
				// Second press ends the gated note early.
				NoteOffMode::Gate(_) => {
					let held = self.held.remove(position);
					events.push(MidiEvent::NoteOff {
						channel: held.channel,
						key: held.note,
					});
					events
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
			pressed_at: now,
		});
		events.push(MidiEvent::NoteOn {
			channel,
			key: note,
			velocity: self.velocity,
		});
		events
	}

	/// Key released (release mode only; ignored in gate mode). The note off is deferred by a debounce
	/// window so that the terminal's auto-repeat pairs do not cut the note: `RELEASE_DEBOUNCE` for a
	/// real release, `PAIRED_RELEASE_DEBOUNCE` for one synthesized right after its press; `tick` emits
	/// the off when the window closes without a press. A second release keeps the original deadline -
	/// only a press cancels it.
	pub fn release(&mut self, key_char: char, now: Instant) -> Vec<MidiEvent> {
		if self.mode != NoteOffMode::Release {
			return Vec::new();
		}
		let lower = key_char.to_ascii_lowercase();
		if let Some(held) = self.held.iter_mut().find(|held| held.key_char == lower)
			&& held.off_at.is_none()
		{
			let is_synthesized = now.saturating_duration_since(held.pressed_at) < SYNTHESIZED_RELEASE_GAP;
			let window = if is_synthesized {
				PAIRED_RELEASE_DEBOUNCE
			} else {
				RELEASE_DEBOUNCE
			};
			held.off_at = Some(now + window);
		}
		Vec::new()
	}

	/// Gate timer and release debounce: note offs whose time has come.
	pub fn tick(&mut self, now: Instant) -> Vec<MidiEvent> {
		self.flush_expired(now)
	}

	/// Note offs whose time has come: the gate deadline or the release debounce window.
	fn flush_expired(&mut self, now: Instant) -> Vec<MidiEvent> {
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
	fn release_mode_defers_note_off_and_cancels_it_on_autorepeat() {
		let mut keyboard = release_keyboard();
		let now = Instant::now();
		let ms = |value: u64| now + Duration::from_millis(value);
		assert_eq!(
			keyboard.press('z', 0, now),
			vec![MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			}]
		);
		assert!(keyboard.press('z', 0, now).is_empty(), "still held without a pending release");
		assert!(keyboard.press('Z', 0, now).is_empty(), "shifted repeat of the same key");
		assert!(keyboard.tick(now + Duration::from_secs(10)).is_empty(), "no gate in release mode");
		// X11 auto-repeat: a Release+Press pair ~2 ms apart — the note-off is cancelled.
		assert!(keyboard.release('z', ms(570)).is_empty(), "note-off is deferred");
		assert!(keyboard.press('z', 0, ms(572)).is_empty(), "the pair cancels the pending note-off");
		assert!(keyboard.tick(ms(580)).is_empty(), "the note keeps sounding");
		// A real release: the window expires with no press following.
		assert!(keyboard.release('z', ms(2000)).is_empty());
		assert!(keyboard.tick(ms(2010)).is_empty(), "still inside the debounce window");
		assert_eq!(keyboard.tick(ms(2025)), vec![MidiEvent::NoteOff { channel: 0, key: 60 }]);
		assert_eq!(keyboard.held_count(), 0);
		assert_eq!(
			keyboard.press('z', 0, ms(3000)),
			vec![MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			}],
			"a later press plays again"
		);
	}

	#[test]
	fn release_mode_repress_past_the_window_flushes_the_old_note_first() {
		let mut keyboard = release_keyboard();
		let now = Instant::now();
		let ms = |value: u64| now + Duration::from_millis(value);
		keyboard.press('z', 0, now);
		keyboard.release('z', ms(500));
		assert_eq!(
			keyboard.press('z', 0, ms(540)),
			vec![
				MidiEvent::NoteOff { channel: 0, key: 60 },
				MidiEvent::NoteOn {
					channel: 0,
					key: 60,
					velocity: 100
				}
			],
			"a re-press past the window re-attacks: off the old note, on the new"
		);
	}

	#[test]
	fn release_mode_bridges_windows_conpty_press_release_pairs() {
		// Timing from a Windows log: every character is a Press+Release pair ~3 ms apart; after the
		// 500 ms initial delay the held key repeats every ~31 ms with occasional ~47 ms gaps.
		let mut keyboard = release_keyboard();
		let now = Instant::now();
		let ms = |value: u64| now + Duration::from_millis(value);
		let on = vec![MidiEvent::NoteOn {
			channel: 0,
			key: 65,
			velocity: 100,
		}];
		let off = vec![MidiEvent::NoteOff { channel: 0, key: 65 }];
		assert_eq!(keyboard.press('v', 0, now), on);
		assert!(keyboard.release('v', ms(3)).is_empty(), "synthesized release is deferred");
		assert!(
			keyboard.tick(ms(50)).is_empty(),
			"the paired window is longer than the real-release one"
		);
		assert_eq!(keyboard.tick(ms(103)), off, "a tap ends when the paired window closes");
		// The hold: the initial delay has passed and the pairs stream in.
		assert_eq!(keyboard.press('v', 0, ms(500)), on);
		let mut press_at = 500;
		for gap in [31, 31, 31, 47, 31, 31, 31, 31, 31, 47, 31, 31] {
			assert!(keyboard.release('v', ms(press_at + 3)).is_empty());
			assert!(
				keyboard.tick(ms(press_at + gap - 1)).is_empty(),
				"tick before the pair at {press_at} + {gap}"
			);
			press_at += gap;
			assert!(keyboard.press('v', 0, ms(press_at)).is_empty(), "repeat pair at {press_at} ms");
		}
		assert_eq!(keyboard.held_count(), 1, "one note for the whole hold");
		// The key goes up: the last pair is not followed by another one.
		assert!(keyboard.release('v', ms(press_at + 3)).is_empty());
		assert!(keyboard.tick(ms(press_at + 90)).is_empty(), "still inside the paired window");
		assert_eq!(keyboard.tick(ms(press_at + 103)), off, "note off once the stream has stopped");
		assert_eq!(keyboard.held_count(), 0);
	}

	#[test]
	fn release_mode_pending_note_off_is_flushed_by_escalation() {
		let mut keyboard = release_keyboard();
		let now = Instant::now();
		let ms = |value: u64| now + Duration::from_millis(value);
		keyboard.press('z', 0, now);
		keyboard.press('s', 0, ms(100));
		keyboard.release('z', ms(200));
		assert_eq!(
			keyboard.all_notes_off(),
			vec![
				MidiEvent::NoteOff { channel: 0, key: 60 },
				MidiEvent::NoteOff { channel: 0, key: 61 }
			],
			"Esc stops a note whose note-off is still waiting out the window"
		);
		assert_eq!(keyboard.held_count(), 0);
	}

	#[test]
	fn gate_mode_times_out_or_ends_on_second_press() {
		let mut keyboard = gate_keyboard();
		let now = Instant::now();
		let ms = |value: u64| now + Duration::from_millis(value);
		assert_eq!(
			keyboard.press('x', 3, now),
			vec![MidiEvent::NoteOn {
				channel: 3,
				key: 62,
				velocity: 100
			}]
		);
		assert!(keyboard.release('x', now).is_empty(), "releases are ignored in gate mode");
		assert!(keyboard.tick(ms(299)).is_empty());
		assert_eq!(keyboard.tick(now + GATE), vec![MidiEvent::NoteOff { channel: 3, key: 62 }]);
		assert_eq!(keyboard.held_count(), 0);
		assert_eq!(
			keyboard.press('x', 3, ms(450)),
			vec![MidiEvent::NoteOn {
				channel: 3,
				key: 62,
				velocity: 100
			}],
			"a press past the guard plays again"
		);
		assert_eq!(
			keyboard.press('x', 3, ms(600)),
			vec![MidiEvent::NoteOff { channel: 3, key: 62 }],
			"second press ends the note"
		);
	}

	#[test]
	fn gate_mode_swallows_a_sustained_autorepeat_but_allows_a_later_repress() {
		let mut keyboard = gate_keyboard();
		let now = Instant::now();
		let ms = |value: u64| now + Duration::from_millis(value);
		assert_eq!(
			keyboard.press('z', 0, now),
			vec![MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			}]
		);
		assert_eq!(keyboard.tick(now + GATE), vec![MidiEvent::NoteOff { channel: 0, key: 60 }]);
		// X11 starts repeating after its initial delay: the first repeat is indistinguishable
		// from a deliberate re-press and plays…
		assert_eq!(
			keyboard.press('z', 0, ms(570)),
			vec![MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			}]
		);
		for offset in (607..=800).step_by(37) {
			assert!(keyboard.press('z', 0, ms(offset)).is_empty(), "repeat at {offset} ms");
		}
		assert_eq!(keyboard.tick(ms(900)), vec![MidiEvent::NoteOff { channel: 0, key: 60 }]);
		assert_eq!(keyboard.held_count(), 0);
		// …but the stream after it never passes the guard, gate deadline or not.
		for offset in (837..=1800).step_by(37) {
			assert!(
				keyboard.press('z', 0, ms(offset)).is_empty(),
				"repeat at {offset} ms after the gate"
			);
		}
		assert_eq!(
			keyboard.press('z', 0, ms(2000)),
			vec![MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			}],
			"the key was really released and pressed again"
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
		assert!(keyboard.release('z', now).is_empty());
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
