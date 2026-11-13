//! Tests of the MIDI file reader on files put together from bytes here: every case names what it holds.

use super::*;

/// Ticks per quarter note of the test files: at the default tempo a quarter note is half a second.
const DIVISION: u16 = 480;
const QUARTER: u32 = 480;

/// A number in the format's variable-length form.
fn number(value: u32) -> Vec<u8> {
	let mut groups = vec![(value & 0x7F) as u8];
	let mut rest = value >> 7;
	while rest > 0 {
		groups.push((rest & 0x7F) as u8 | 0x80);
		rest >>= 7;
	}
	groups.reverse();
	groups
}

/// An event: the ticks since the previous one, then its bytes.
fn after(delta: u32, bytes: &[u8]) -> Vec<u8> {
	let mut event = number(delta);
	event.extend_from_slice(bytes);
	event
}

fn tempo(delta: u32, microseconds: u32) -> Vec<u8> {
	let value = microseconds.to_be_bytes();
	after(delta, &[0xFF, 0x51, 3, value[1], value[2], value[3]])
}

fn end_of_track(delta: u32) -> Vec<u8> {
	after(delta, &[0xFF, 0x2F, 0])
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
	let mut bytes = kind.to_vec();
	bytes.extend((data.len() as u32).to_be_bytes());
	bytes.extend_from_slice(data);
	bytes
}

fn track(events: &[Vec<u8>]) -> Vec<u8> {
	chunk(b"MTrk", &events.concat())
}

fn header(format: u16, tracks: u16, division: u16) -> Vec<u8> {
	let mut data = Vec::new();
	data.extend(format.to_be_bytes());
	data.extend(tracks.to_be_bytes());
	data.extend(division.to_be_bytes());
	chunk(b"MThd", &data)
}

fn file(format: u16, division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
	let mut bytes = header(format, tracks.len() as u16, division);
	for track in tracks {
		bytes.extend_from_slice(track);
	}
	bytes
}

fn on(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOn {
		channel,
		key,
		velocity: 100,
	}
}

fn off(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOff { channel, key }
}

/// The events of a song as (seconds, event) pairs.
fn timeline(song: &Song) -> Vec<(f64, MidiEvent)> {
	song.events.iter().map(|timed| (timed.seconds, timed.event)).collect()
}

fn parsed(bytes: &[u8]) -> Song {
	parse(bytes).unwrap_or_else(|error| panic!("the file should parse: {error}"))
}

#[test]
fn a_format_0_file_gives_its_messages_with_their_times_in_seconds() {
	let bytes = file(
		0,
		DIVISION,
		&[track(&[
			after(0, &[0xC0, 5]),
			after(0, &[0x90, 60, 100]),
			after(QUARTER, &[0x80, 60, 0]),
			after(QUARTER / 2, &[0x91, 64, 100]),
			after(QUARTER * 2, &[0x81, 64, 64]),
			after(0, &[0xE0, 0x00, 0x60]),
			after(0, &[0xB0, 7, 90]),
			after(0, &[0xD2, 33]),
			end_of_track(0),
		])],
	);
	let song = parsed(&bytes);
	assert_eq!(
		timeline(&song),
		[
			(0.0, MidiEvent::ProgramChange { channel: 0, program: 5 }),
			(0.0, on(0, 60)),
			(0.5, off(0, 60)),
			(0.75, on(1, 64)),
			(1.75, off(1, 64)),
			(1.75, MidiEvent::PitchBend { channel: 0, value: 0x3000 }),
			(
				1.75,
				MidiEvent::ControlChange {
					channel: 0,
					controller: 7,
					value: 90
				}
			),
			(1.75, MidiEvent::ChannelPressure { channel: 2, pressure: 33 }),
		]
	);
	assert_eq!(song.duration, 1.75);
}

#[test]
fn a_message_without_a_status_byte_reuses_the_status_of_the_one_before() {
	let bytes = file(
		0,
		DIVISION,
		&[track(&[
			after(0, &[0x90, 60, 100]),
			// The same Note On status, left out: two more notes, then two "notes" of velocity 0 —
			// the usual way to end notes without changing the status.
			after(0, &[64, 100]),
			after(0, &[67, 100]),
			after(QUARTER, &[60, 0]),
			after(0, &[64, 0]),
			// A program change has one data byte, and its status runs on as well.
			after(0, &[0xC3, 10]),
			after(QUARTER, &[11]),
			end_of_track(0),
		])],
	);
	assert_eq!(
		timeline(&parsed(&bytes)),
		[
			(0.0, on(0, 60)),
			(0.0, on(0, 64)),
			(0.0, on(0, 67)),
			(0.5, off(0, 60)),
			(0.5, off(0, 64)),
			(0.5, MidiEvent::ProgramChange { channel: 3, program: 10 }),
			(1.0, MidiEvent::ProgramChange { channel: 3, program: 11 }),
		]
	);
}

#[test]
fn the_tracks_of_a_format_1_file_are_merged_in_time_order() {
	let bytes = file(
		1,
		DIVISION,
		&[
			track(&[
				after(QUARTER, &[0x90, 60, 100]),
				after(QUARTER * 2, &[0x80, 60, 0]),
				end_of_track(0),
			]),
			track(&[
				after(0, &[0x91, 40, 100]),
				after(QUARTER, &[0x81, 40, 0]),
				after(QUARTER, &[0x91, 43, 100]),
				end_of_track(QUARTER * 4),
			]),
		],
	);
	let song = parsed(&bytes);
	assert_eq!(
		timeline(&song),
		[
			(0.0, on(1, 40)),
			// Due at the same tick: the first track's event comes first.
			(0.5, on(0, 60)),
			(0.5, off(1, 40)),
			(1.0, on(1, 43)),
			(1.5, off(0, 60)),
		]
	);
	assert_eq!(
		song.duration, 3.0,
		"the file lasts as long as its longest track, silence at the end included"
	);
}

#[test]
fn a_tempo_change_sets_the_length_of_the_ticks_after_it_in_every_track() {
	let bytes = file(
		1,
		DIVISION,
		&[
			// The tempo track: 120 a minute by default, 240 from the second quarter, 60 from the fourth.
			track(&[tempo(QUARTER, 250_000), tempo(QUARTER * 2, 1_000_000), end_of_track(0)]),
			track(&[
				after(0, &[0x90, 60, 100]),
				after(QUARTER, &[0x80, 60, 0]),
				after(QUARTER, &[0x90, 62, 100]),
				after(QUARTER, &[0x80, 62, 0]),
				after(QUARTER, &[0x90, 64, 100]),
				after(QUARTER / 2, &[0x80, 64, 0]),
				end_of_track(0),
			]),
		],
	);
	let song = parsed(&bytes);
	assert_eq!(
		timeline(&song),
		[
			(0.0, on(0, 60)),
			(0.5, off(0, 60)),
			(0.75, on(0, 62)),
			(1.0, off(0, 62)),
			(2.0, on(0, 64)),
			(2.5, off(0, 64)),
		]
	);
	assert_eq!(song.duration, 2.5);

	// A tempo set at the very start replaces the default one from the first tick.
	let slow = file(
		0,
		DIVISION,
		&[track(&[
			tempo(0, 1_000_000),
			after(QUARTER, &[0x90, 60, 100]),
			end_of_track(QUARTER),
		])],
	);
	let song = parsed(&slow);
	assert_eq!(timeline(&song), [(1.0, on(0, 60))]);
	assert_eq!(song.duration, 2.0);
}

#[test]
fn the_division_of_the_header_sets_how_many_ticks_a_quarter_note_has() {
	for (division, expected) in [(96u16, 2.5), (480, 0.5), (960, 0.25)] {
		let bytes = file(0, division, &[track(&[after(480, &[0x90, 60, 100]), end_of_track(0)])]);
		assert_eq!(timeline(&parsed(&bytes)), [(expected, on(0, 60))], "division {division}");
	}
}

#[test]
fn what_is_not_a_channel_message_is_skipped() {
	let mut bytes = header(1, 2, DIVISION);
	// A chunk of a kind the reader does not know sits between the header and the tracks.
	bytes.extend(chunk(b"XFIH", &[1, 2, 3, 4, 5]));
	bytes.extend(track(&[
		after(0, &[0xFF, 0x03, 5, b'P', b'i', b'a', b'n', b'o']),
		after(0, &[0xFF, 0x58, 4, 4, 2, 24, 8]),
		after(0, &[0xF0, 5, 0x7E, 0x7F, 0x09, 0x01, 0xF7]),
		after(QUARTER, &[0x90, 60, 100]),
		// Polyphonic aftertouch: a channel message the synthesizer does not act on.
		after(0, &[0xA0, 60, 50]),
		after(0, &[0xF7, 2, 0x01, 0x02]),
		after(QUARTER, &[0x80, 60, 0]),
		end_of_track(0),
	]));
	bytes.extend(chunk(b"junk", &[]));
	bytes.extend(track(&[end_of_track(0)]));
	let song = parsed(&bytes);
	assert_eq!(timeline(&song), [(0.5, on(0, 60)), (1.0, off(0, 60))]);
}

#[test]
fn a_track_ends_at_its_end_of_track_event_or_where_its_data_ends() {
	// Bytes after End of Track are not events.
	let with_tail = file(
		0,
		DIVISION,
		&[track(&[
			after(0, &[0x90, 60, 100]),
			end_of_track(QUARTER),
			vec![0x00, 0x90, 61, 100],
		])],
	);
	let song = parsed(&with_tail);
	assert_eq!(timeline(&song), [(0.0, on(0, 60))]);
	assert_eq!(song.duration, 0.5);

	// No End of Track at all: the track ends with its last event.
	let unfinished = file(0, DIVISION, &[track(&[after(QUARTER, &[0x90, 60, 100])])]);
	let song = parsed(&unfinished);
	assert_eq!(timeline(&song), [(0.5, on(0, 60))]);
	assert_eq!(song.duration, 0.5);
	// The last event counts even when it is one the reader skips: a marker a quarter note after the note.
	let ends_with_a_marker = file(
		0,
		DIVISION,
		&[track(&[after(0, &[0x90, 60, 100]), after(QUARTER, &[0xFF, 0x06, 1, b'x'])])],
	);
	let song = parsed(&ends_with_a_marker);
	assert_eq!(timeline(&song), [(0.0, on(0, 60))]);
	assert_eq!(song.duration, 0.5);

	// An empty file of the right shape is a song of no length.
	let empty = parsed(&file(0, DIVISION, &[track(&[])]));
	assert!(empty.events.is_empty());
	assert_eq!(empty.duration, 0.0);
	let no_tracks = parsed(&header(1, 0, DIVISION));
	assert!(no_tracks.events.is_empty());
}

#[test]
fn a_header_longer_than_six_bytes_is_read_and_its_rest_skipped() {
	let mut data = Vec::new();
	data.extend(0u16.to_be_bytes());
	data.extend(1u16.to_be_bytes());
	data.extend(DIVISION.to_be_bytes());
	data.extend([0xAA, 0xBB]);
	let mut bytes = chunk(b"MThd", &data);
	bytes.extend(track(&[after(0, &[0x90, 60, 100]), end_of_track(0)]));
	assert_eq!(timeline(&parsed(&bytes)), [(0.0, on(0, 60))]);
}

#[test]
fn long_waits_are_read_from_numbers_of_up_to_four_bytes() {
	assert_eq!(number(0), [0x00]);
	assert_eq!(number(0x7F), [0x7F]);
	assert_eq!(number(0x80), [0x81, 0x00]);
	assert_eq!(number(0x0FFF_FFFF), [0xFF, 0xFF, 0xFF, 0x7F]);
	// 0x0FFFFFFF ticks, the longest wait the format can write, at one tick a quarter note.
	let bytes = file(0, 1, &[track(&[after(0x0FFF_FFFF, &[0x90, 60, 100]), end_of_track(0)])]);
	assert_eq!(timeline(&parsed(&bytes)), [(f64::from(0x0FFF_FFFF) * 0.5, on(0, 60))]);
	// A fifth byte with the "more follows" bit is not a number of this format.
	let bytes = file(0, DIVISION, &[track(&[vec![0x81, 0x80, 0x80, 0x80, 0x00, 0x90, 60, 100]])]);
	assert!(matches!(parse(&bytes), Err(SmfError::NumberTooLong { track: 0 })));
}

#[test]
fn files_of_the_wrong_kind_are_refused_with_the_reason() {
	let not_midi = [
		Vec::new(),
		b"RIFF".to_vec(),
		b"MThd".to_vec()[..3].to_vec(),
		b"MTrk\0\0\0\0".to_vec(),
	];
	for bytes in not_midi {
		assert!(matches!(parse(&bytes), Err(SmfError::NotMidi)), "{bytes:?}");
	}
	for format in [2, 3, 0xFFFF] {
		let bytes = file(format, DIVISION, &[track(&[end_of_track(0)])]);
		assert!(matches!(parse(&bytes), Err(SmfError::UnsupportedFormat(found)) if found == format));
	}
	// 25 frames a second, 40 ticks a frame: time in frames, not in quarter notes.
	let smpte = file(0, 0xE728, &[track(&[end_of_track(0)])]);
	assert!(matches!(parse(&smpte), Err(SmfError::SmpteTiming)));
	let no_ticks = file(0, 0, &[track(&[end_of_track(0)])]);
	assert!(matches!(parse(&no_ticks), Err(SmfError::NoTicks)));
	// A header that declares less data than the format needs.
	let short_header = chunk(b"MThd", &[0, 0, 0, 1]);
	assert!(matches!(parse(&short_header), Err(SmfError::Truncated("the header"))));
}

#[test]
fn broken_tracks_are_refused_with_the_track_and_the_reason() {
	let in_second_track = |events: &[Vec<u8>]| file(1, DIVISION, &[track(&[end_of_track(0)]), track(events)]);

	let zero_tempo = in_second_track(&[tempo(0, 0), end_of_track(0)]);
	assert!(matches!(parse(&zero_tempo), Err(SmfError::ZeroTempo { track: 1 })));

	// A data byte with nothing before it to take the status from.
	let no_status = in_second_track(&[after(0, &[60, 100])]);
	assert!(matches!(parse(&no_status), Err(SmfError::NoStatus { track: 1, byte: 60 })));
	// A meta event ends the running status: the message after it must name its own.
	let after_meta = in_second_track(&[after(0, &[0x90, 60, 100]), after(0, &[0xFF, 0x01, 0]), after(0, &[62, 100])]);
	assert!(matches!(parse(&after_meta), Err(SmfError::NoStatus { track: 1, byte: 62 })));
	let after_sysex = in_second_track(&[after(0, &[0x90, 60, 100]), after(0, &[0xF0, 1, 0xF7]), after(0, &[62, 100])]);
	assert!(matches!(parse(&after_sysex), Err(SmfError::NoStatus { .. })));

	// Real-time and system common messages belong on a wire, not in a file.
	for status in [0xF1, 0xF2, 0xF3, 0xF6, 0xF8, 0xFE] {
		let bytes = in_second_track(&[after(0, &[status, 0, 0])]);
		assert!(matches!(parse(&bytes), Err(SmfError::BadStatus { track: 1, status: found }) if found == status));
	}
	// A status byte where a data byte belongs.
	let bad_data = in_second_track(&[after(0, &[0x90, 60, 0x90])]);
	assert!(matches!(parse(&bad_data), Err(SmfError::BadMessage { track: 1, status: 0x90 })));

	let message = parse(&zero_tempo).expect_err("refused").to_string();
	assert_eq!(message, "track 1: a tempo of zero");
}

#[test]
fn a_file_cut_short_anywhere_is_an_error_and_never_a_panic() {
	let whole = file(
		1,
		DIVISION,
		&[
			track(&[
				tempo(0, 400_000),
				after(0, &[0xFF, 0x03, 4, b'T', b'e', b's', b't']),
				end_of_track(0),
			]),
			track(&[
				after(0, &[0x90, 60, 100]),
				after(QUARTER, &[64, 100]),
				after(200, &[0xF0, 3, 1, 2, 0xF7]),
				after(QUARTER, &[0x80, 60, 0]),
				after(0, &[0x80, 64, 0]),
				end_of_track(0),
			]),
		],
	);
	assert_eq!(parsed(&whole).events.len(), 4);
	for length in 0..whole.len() {
		let cut = &whole[..length];
		assert!(
			parse(cut).is_err(),
			"the first {length} of {} bytes are not a whole file",
			whole.len()
		);
	}
	// A track that declares more data than the file has.
	let mut lying = file(0, DIVISION, &[]);
	lying[11] = 1;
	lying.extend(b"MTrk");
	lying.extend(1000u32.to_be_bytes());
	lying.extend([0x00, 0x90, 60, 100]);
	assert!(matches!(parse(&lying), Err(SmfError::Truncated("a track"))));
	// The header promises two tracks, the file has one.
	let mut one_short = file(1, DIVISION, &[track(&[end_of_track(0)])]);
	one_short[11] = 2;
	assert!(matches!(parse(&one_short), Err(SmfError::Truncated("a chunk header"))));
}

#[test]
fn arbitrary_bytes_after_a_valid_header_never_panic() {
	// A fixed sequence of pseudo-random bytes: the same on every run.
	let mut state = 0x2545_F491_4F6C_DD1Du64;
	let mut next = move || {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		(state >> 24) as u8
	};
	for round in 0..2_000 {
		let length = usize::from(next()) % 96;
		let noise: Vec<u8> = (0..length).map(|_| next()).collect();
		let bytes = file(u16::from(next() % 2), DIVISION, &[chunk(b"MTrk", &noise)]);
		// Whatever comes out is fine; getting here without a panic is the point.
		if let Ok(song) = parse(&bytes) {
			assert!(song.duration.is_finite() && song.duration >= 0.0, "round {round}");
			assert!(
				song.events.windows(2).all(|pair| pair[0].seconds <= pair[1].seconds),
				"round {round}"
			);
		}
	}
}

#[test]
fn a_file_is_loaded_from_disk_and_a_missing_one_names_its_path() {
	let directory = std::env::temp_dir().join(format!("sf2_player-smf-test-{}", std::process::id()));
	std::fs::create_dir_all(&directory).expect("scratch directory");
	let path = directory.join("two notes.mid");
	let bytes = file(
		0,
		DIVISION,
		&[track(&[
			after(0, &[0x90, 60, 100]),
			after(QUARTER, &[0x80, 60, 0]),
			end_of_track(0),
		])],
	);
	std::fs::write(&path, &bytes).expect("write");
	assert_eq!(load(&path).expect("loads"), parsed(&bytes));

	let missing = directory.join("not here.mid");
	let error = load(&missing).expect_err("no such file");
	assert!(matches!(error, SmfError::Read { .. }));
	assert!(error.to_string().contains("not here.mid"), "{error}");
	std::fs::remove_dir_all(&directory).ok();
}
