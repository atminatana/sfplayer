//! Writing Standard MIDI Files of format 0 (spec v1.2, F-23): the pieces a file is put together from.
//!
//! The files written here keep the tempo every file starts with — 120 quarter notes a minute — and
//! count 480 ticks to a quarter note, so a tick is always 1/960 of a second and time needs no tempo
//! changes. Who writes the file decides how it reaches the disk; nothing here touches one.

use super::{DEFAULT_TEMPO_MICROSECONDS, HEADER_CHUNK, HEADER_LENGTH, META_END_OF_TRACK, META_EVENT, META_TEMPO, TRACK_CHUNK};
use crate::midi::MidiEvent;

/// Ticks per quarter note.
pub const DIVISION: u16 = 480;
/// What `DIVISION` and the tempo come to.
pub const TICKS_PER_SECOND: u64 = DIVISION as u64 * 1_000_000 / DEFAULT_TEMPO_MICROSECONDS as u64;
/// Where in the file the length of the track stands: it is known only when the track is complete.
pub const TRACK_LENGTH_OFFSET: u64 = 18;
/// The longest wait a variable-length number of four bytes can hold.
const LONGEST_WAIT: u32 = 0x0FFF_FFFF;
const FORMAT_SINGLE_TRACK: u16 = 0;

/// The header of a one-track file and the head of that track, for a track of `track_length` bytes.
pub fn file_head(track_length: u32) -> Vec<u8> {
	let mut head = Vec::with_capacity(22);
	head.extend_from_slice(HEADER_CHUNK);
	head.extend((HEADER_LENGTH as u32).to_be_bytes());
	head.extend(FORMAT_SINGLE_TRACK.to_be_bytes());
	head.extend(1u16.to_be_bytes());
	head.extend(DIVISION.to_be_bytes());
	head.extend_from_slice(TRACK_CHUNK);
	head.extend(track_length.to_be_bytes());
	head
}

/// The tick of audio frame `frame` at `sample_rate`, rounded to the nearest.
pub fn tick_of(frame: u64, sample_rate: u32) -> u64 {
	let rate = u128::from(sample_rate.max(1));
	let ticks = (u128::from(frame) * u128::from(TICKS_PER_SECOND) + rate / 2) / rate;
	u64::try_from(ticks).unwrap_or(u64::MAX)
}

/// A number in the format's variable-length form: seven bits a byte, the high bit set on all but the last.
pub fn push_variable_length(out: &mut Vec<u8>, value: u32) {
	let value = value.min(LONGEST_WAIT);
	for shift in [21, 14, 7] {
		if value >> shift != 0 {
			out.push(((value >> shift) & 0x7F) as u8 | 0x80);
		}
	}
	out.push((value & 0x7F) as u8);
}

/// The tempo the files written here are counted in, stated at the start of the track.
pub fn push_tempo(out: &mut Vec<u8>) {
	let tempo = DEFAULT_TEMPO_MICROSECONDS.to_be_bytes();
	push_variable_length(out, 0);
	out.extend([META_EVENT, META_TEMPO, 3, tempo[1], tempo[2], tempo[3]]);
}

pub fn push_end_of_track(out: &mut Vec<u8>, delta: u32) {
	push_variable_length(out, delta);
	out.extend([META_EVENT, META_END_OF_TRACK, 0]);
}

/// A channel message `delta` ticks after the previous event. Returns `false`, writing nothing, for
/// what is not a channel message a file of this program holds.
pub fn push_message(out: &mut Vec<u8>, delta: u32, event: MidiEvent) -> bool {
	let low = |value: u8| value & 0x7F;
	let status = |kind: u8, channel: u8| kind | (channel & 0x0F);
	let (bytes, length) = match event {
		MidiEvent::NoteOn { channel, key, velocity } => ([status(0x90, channel), low(key), low(velocity)], 3),
		MidiEvent::NoteOff { channel, key } => ([status(0x80, channel), low(key), 0], 3),
		MidiEvent::ControlChange {
			channel,
			controller,
			value,
		} => ([status(0xB0, channel), low(controller), low(value)], 3),
		MidiEvent::ProgramChange { channel, program } => ([status(0xC0, channel), low(program), 0], 2),
		MidiEvent::ChannelPressure { channel, pressure } => ([status(0xD0, channel), low(pressure), 0], 2),
		MidiEvent::PitchBend { channel, value } => ([status(0xE0, channel), (value & 0x7F) as u8, ((value >> 7) & 0x7F) as u8], 3),
		MidiEvent::Other { .. } => return false,
	};
	push_variable_length(out, delta);
	out.extend_from_slice(&bytes[..length]);
	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::midi::smf::parse;

	fn number(value: u32) -> Vec<u8> {
		let mut out = Vec::new();
		push_variable_length(&mut out, value);
		out
	}

	#[test]
	fn numbers_take_as_many_bytes_as_they_need() {
		assert_eq!(number(0), [0x00]);
		assert_eq!(number(0x7F), [0x7F]);
		assert_eq!(number(0x80), [0x81, 0x00]);
		assert_eq!(number(0x3FFF), [0xFF, 0x7F]);
		assert_eq!(number(0x4000), [0x81, 0x80, 0x00]);
		assert_eq!(number(0x0FFF_FFFF), [0xFF, 0xFF, 0xFF, 0x7F]);
		assert_eq!(
			number(u32::MAX),
			[0xFF, 0xFF, 0xFF, 0x7F],
			"longer waits are cut to the longest the format holds"
		);
	}

	#[test]
	fn a_tick_is_one_960th_of_a_second() {
		assert_eq!(TICKS_PER_SECOND, 960);
		assert_eq!(tick_of(0, 48_000), 0);
		assert_eq!(tick_of(48_000, 48_000), 960);
		assert_eq!(tick_of(44_100 * 3600, 44_100), 960 * 3600, "an hour in does not drift");
		assert_eq!(tick_of(24, 48_000), 0, "just under half a tick");
		assert_eq!(tick_of(25, 48_000), 1, "half a tick rounds up");
		assert_eq!(tick_of(100, 0), 96_000, "a rate of zero does not divide by zero");
	}

	#[test]
	fn every_channel_message_written_is_read_back_by_the_reader() {
		let events = [
			MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			},
			MidiEvent::ControlChange {
				channel: 15,
				controller: 64,
				value: 127,
			},
			MidiEvent::ProgramChange { channel: 9, program: 5 },
			MidiEvent::ChannelPressure { channel: 3, pressure: 77 },
			MidiEvent::PitchBend { channel: 1, value: 16_383 },
			MidiEvent::PitchBend { channel: 1, value: 8_192 },
			MidiEvent::NoteOff { channel: 0, key: 60 },
		];
		let mut track = Vec::new();
		push_tempo(&mut track);
		for (index, event) in events.iter().enumerate() {
			// The first at the start, each next one 480 ticks — half a second — later.
			assert!(push_message(&mut track, if index == 0 { 0 } else { 480 }, *event));
		}
		assert!(
			!push_message(&mut track, 5, MidiEvent::Other { status: 0xA0 }),
			"not a message a file holds"
		);
		push_end_of_track(&mut track, 960);
		let mut file = file_head(track.len() as u32);
		assert_eq!(file.len() as u64, TRACK_LENGTH_OFFSET + 4, "the length of the track ends the head");
		file.extend(track);

		let song = parse(&file).expect("the reader accepts what is written here");
		let read: Vec<MidiEvent> = song.events.iter().map(|timed| timed.event).collect();
		assert_eq!(read, events);
		let seconds: Vec<f64> = song.events.iter().map(|timed| timed.seconds).collect();
		assert_eq!(seconds, [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0]);
		assert_eq!(song.duration, 4.0, "the end of the track is a second after the last message");
	}
}
