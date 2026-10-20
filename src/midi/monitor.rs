//! MIDI monitor entries (spec F-08): a fixed-size record produced on the MIDI thread and formatted on the UI thread.

use std::time::SystemTime;

use super::MidiEvent;
use super::parse::PITCH_BEND_CENTRE;

/// One monitored message. `Copy` and allocation-free so it can travel through an SPSC ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorEntry {
	pub at: SystemTime,
	pub event: MidiEvent,
}

impl MonitorEntry {
	pub fn now(event: MidiEvent) -> Self {
		Self {
			at: SystemTime::now(),
			event,
		}
	}

	/// "12:03:41.120  ch01 NoteOn   C4  vel 92" — the layout of the spec §6 mock-up.
	pub fn format(&self) -> String {
		let time = chrono::DateTime::<chrono::Local>::from(self.at).format("%H:%M:%S%.3f");
		format!("{time}  {}", describe(&self.event))
	}
}

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Scientific pitch name with middle C = C4 (key 60), so key 0 is C-1 and key 127 is G9.
pub fn note_name(key: u8) -> String {
	let octave = i32::from(key / 12) - 1;
	format!("{}{octave}", NOTE_NAMES[usize::from(key % 12)])
}

/// Message description without the timestamp. Channels are shown 1-based.
pub fn describe(event: &MidiEvent) -> String {
	match *event {
		MidiEvent::NoteOn { channel, key, velocity } => format!("ch{:02} NoteOn   {:<4} vel {velocity}", channel + 1, note_name(key)),
		MidiEvent::NoteOff { channel, key } => format!("ch{:02} NoteOff  {:<4}", channel + 1, note_name(key)),
		MidiEvent::ControlChange {
			channel,
			controller,
			value,
		} => format!("ch{:02} CC {controller:<3}   {value}", channel + 1),
		MidiEvent::ProgramChange { channel, program } => format!("ch{:02} Program  {program}", channel + 1),
		MidiEvent::PitchBend { channel, value } => {
			format!(
				"ch{:02} PitchBend {:+}",
				channel + 1,
				i32::from(value) - i32::from(PITCH_BEND_CENTRE)
			)
		}
		MidiEvent::ChannelPressure { channel, pressure } => format!("ch{:02} Pressure {pressure}", channel + 1),
		MidiEvent::Other { status } if status < 0xF0 => format!("ch{:02} Other    {:#04x}", (status & 0x0F) + 1, status),
		MidiEvent::Other { status } => format!("     System   {status:#04x}"),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn note_names_follow_scientific_pitch() {
		assert_eq!(note_name(60), "C4");
		assert_eq!(note_name(61), "C#4");
		assert_eq!(note_name(69), "A4");
		assert_eq!(note_name(0), "C-1");
		assert_eq!(note_name(127), "G9");
	}

	#[test]
	fn descriptions_match_the_mockup_layout() {
		assert_eq!(
			describe(&MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 92
			}),
			"ch01 NoteOn   C4   vel 92"
		);
		assert_eq!(describe(&MidiEvent::NoteOff { channel: 0, key: 61 }), "ch01 NoteOff  C#4 ");
		assert_eq!(
			describe(&MidiEvent::ControlChange {
				channel: 0,
				controller: 64,
				value: 127
			}),
			"ch01 CC 64    127"
		);
		assert_eq!(
			describe(&MidiEvent::PitchBend {
				channel: 0,
				value: 8192 + 1024
			}),
			"ch01 PitchBend +1024"
		);
		assert_eq!(describe(&MidiEvent::PitchBend { channel: 15, value: 0 }), "ch16 PitchBend -8192");
		assert_eq!(describe(&MidiEvent::ProgramChange { channel: 9, program: 5 }), "ch10 Program  5");
		assert_eq!(
			describe(&MidiEvent::ChannelPressure { channel: 0, pressure: 77 }),
			"ch01 Pressure 77"
		);
		assert_eq!(describe(&MidiEvent::Other { status: 0xA3 }), "ch04 Other    0xa3");
		assert_eq!(describe(&MidiEvent::Other { status: 0xF8 }), "     System   0xf8");
	}

	#[test]
	fn formatted_entry_starts_with_a_millisecond_timestamp() {
		let text = MonitorEntry::now(MidiEvent::NoteOff { channel: 0, key: 60 }).format();
		let (time, rest) = text.split_once("  ").expect("two spaces after the time");
		assert_eq!(time.len(), "12:03:41.120".len(), "{time}");
		assert!(time.as_bytes()[2] == b':' && time.as_bytes()[8] == b'.');
		assert_eq!(rest, "ch01 NoteOff  C4  ");
	}
}
