//! A MIDI take is read back with the program's own reader: the events and the time between them are
//! what was written.

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::midi::smf::{self, Song};

const SAMPLE_RATE: u32 = 48_000;
static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_path() -> PathBuf {
	let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
	std::env::temp_dir().join(format!("sf2_player-midi-take-{}-{unique}.mid", std::process::id()))
}

fn note_on(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOn {
		channel,
		key,
		velocity: 100,
	}
}

fn note_off(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOff { channel, key }
}

/// An event `seconds` into the take.
fn at(seconds: f64, event: MidiEvent) -> StampedEvent {
	StampedEvent {
		frame: (seconds * f64::from(SAMPLE_RATE)) as u64,
		event,
	}
}

/// Writes `events`, ends the take `end_seconds` in and reads the file back.
fn written_and_read(events: &[StampedEvent], end_seconds: u64) -> Song {
	let path = temp_path();
	let mut take = MidiTake::create(&path, SAMPLE_RATE).expect("create");
	for event in events {
		take.write(*event).expect("write");
	}
	take.finish_at(end_seconds * TICKS_PER_SECOND).expect("finish");
	let song = smf::load(&path).expect("the reader accepts the take");
	std::fs::remove_file(&path).ok();
	song
}

fn read_back(song: &Song) -> Vec<(f64, MidiEvent)> {
	song.events.iter().map(|timed| (timed.seconds, timed.event)).collect()
}

#[test]
fn a_take_reads_back_as_the_events_fed_and_the_time_between_them() {
	let bend = MidiEvent::PitchBend { channel: 2, value: 12_000 };
	let pedal = MidiEvent::ControlChange {
		channel: 0,
		controller: 64,
		value: 127,
	};
	let program = MidiEvent::ProgramChange { channel: 9, program: 40 };
	let song = written_and_read(
		&[
			at(0.25, program),
			at(0.5, note_on(0, 60)),
			at(0.5, note_on(15, 127)),
			at(1.0, pedal),
			at(1.125, bend),
			at(2.0, note_off(0, 60)),
			at(2.0, MidiEvent::Other { status: 0xF8 }),
			at(3.5, note_off(15, 127)),
		],
		5,
	);
	assert_eq!(
		read_back(&song),
		[
			(0.25, program),
			(0.5, note_on(0, 60)),
			(0.5, note_on(15, 127)),
			(1.0, pedal),
			(1.125, bend),
			(2.0, note_off(0, 60)),
			(3.5, note_off(15, 127)),
		],
		"the silence before the first event is kept; what is not a channel message is left out"
	);
	assert_eq!(song.duration, 5.0, "the file is as long as the take");
}

#[test]
fn notes_still_sounding_when_the_take_ends_are_ended_in_the_file() {
	let song = written_and_read(
		&[
			at(1.0, note_on(0, 60)),
			at(1.0, note_on(3, 72)),
			at(1.5, note_on(0, 64)),
			at(2.0, note_off(0, 64)),
			// A note on with no velocity is a note off.
			at(2.5, note_on(0, 67)),
			at(
				2.75,
				MidiEvent::NoteOn {
					channel: 0,
					key: 67,
					velocity: 0,
				},
			),
		],
		4,
	);
	let tail: Vec<(f64, MidiEvent)> = read_back(&song).into_iter().skip(6).collect();
	assert_eq!(tail, [(4.0, note_off(0, 60)), (4.0, note_off(3, 72))], "only the two left sounding");
	assert_eq!(song.duration, 4.0);
}

#[test]
fn an_end_before_the_last_event_does_not_turn_time_back() {
	let song = written_and_read(&[at(3.0, note_on(0, 60))], 1);
	assert_eq!(read_back(&song), [(3.0, note_on(0, 60)), (3.0, note_off(0, 60))]);
	assert_eq!(song.duration, 3.0);
}

#[test]
fn an_event_stamped_before_the_previous_one_is_written_at_its_time() {
	let song = written_and_read(&[at(2.0, note_on(0, 60)), at(1.0, note_off(0, 60))], 2);
	assert_eq!(read_back(&song), [(2.0, note_on(0, 60)), (2.0, note_off(0, 60))]);
	assert_eq!(song.duration, 2.0, "and what follows is counted from where the file is");
}

#[test]
fn an_empty_take_is_a_valid_file_of_its_length() {
	let song = written_and_read(&[], 7);
	assert!(song.events.is_empty());
	assert_eq!(song.duration, 7.0);
}

#[test]
fn the_file_grows_on_the_disk_while_the_take_runs_and_names_its_track_length() {
	let path = temp_path();
	let mut take = MidiTake::create(&path, SAMPLE_RATE).expect("create");
	// More events than the writer keeps in memory: 3000 notes of 4 bytes each.
	for index in 0..3_000u32 {
		take.write(at(f64::from(index) * 0.001, note_on(0, (index % 128) as u8)))
			.expect("write");
	}
	let while_running = std::fs::metadata(&path).expect("the file").len();
	assert!(while_running > 0, "events reach the disk before the take ends");
	take.finish_at(10 * TICKS_PER_SECOND).expect("finish");
	let bytes = std::fs::read(&path).expect("read back");
	let declared = u32::from_be_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as usize;
	assert_eq!(declared, bytes.len() - 22, "the track is as long as its head says");
	assert_eq!(&bytes[bytes.len() - 3..], [0xFF, 0x2F, 0x00], "and ends with End of Track");
	std::fs::remove_file(&path).ok();
}

#[test]
fn a_take_in_a_folder_that_is_not_there_is_a_create_error() {
	let path = PathBuf::from("definitely/not/here/take.mid");
	assert!(matches!(MidiTake::create(&path, SAMPLE_RATE), Err(RecordError::Create { .. })));
}
