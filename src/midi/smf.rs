//! Reading Standard MIDI Files (spec v1.2, F-21): formats 0 and 1, turned into one list of channel
//! messages with the time of each in seconds from the start.
//!
//! The file gives time in ticks and the length of a tick through tempo changes; both are resolved
//! here, so the player needs no knowledge of either. Tracks are merged in time order. Whatever is not a
//! channel message the synthesizer acts on — system exclusive data, texts, markers — is skipped.

use std::path::{Path, PathBuf};

use super::parse::{MidiEvent, parse_message};

const HEADER_CHUNK: &[u8; 4] = b"MThd";
const TRACK_CHUNK: &[u8; 4] = b"MTrk";
/// Length of the header data the format defines.
const HEADER_LENGTH: usize = 6;
/// Tempo until the file sets one: 120 quarter notes a minute.
const DEFAULT_TEMPO_MICROSECONDS: u32 = 500_000;
const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;
/// A variable-length number takes at most four bytes.
const VARIABLE_LENGTH_BYTES: usize = 4;
const META_EVENT: u8 = 0xFF;
const META_END_OF_TRACK: u8 = 0x2F;
const META_TEMPO: u8 = 0x51;
const SYSTEM_EXCLUSIVE: u8 = 0xF0;
const SYSTEM_EXCLUSIVE_CONTINUED: u8 = 0xF7;

/// A channel message and when it is due, in seconds from the start of the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedEvent {
	pub seconds: f64,
	pub event: MidiEvent,
}

/// A MIDI file ready to be played.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
	/// In the order they are due; events due at the same moment keep the order of their tracks.
	pub events: Vec<TimedEvent>,
	/// Where the longest track ends, in seconds.
	pub duration: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum SmfError {
	#[error("cannot read {path}: {source}")]
	Read {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("not a MIDI file: it does not start with the MThd header")]
	NotMidi,
	#[error("the file ends in the middle of {0}")]
	Truncated(&'static str),
	#[error("MIDI file format {0} is not supported (only 0 and 1 are)")]
	UnsupportedFormat(u16),
	#[error("time in frames per second (SMPTE) is not supported, only ticks per quarter note")]
	SmpteTiming,
	#[error("the file declares no ticks per quarter note")]
	NoTicks,
	#[error("track {track}: a number is longer than four bytes")]
	NumberTooLong { track: usize },
	#[error("track {track}: data byte {byte:#04x} without a status byte before it")]
	NoStatus { track: usize, byte: u8 },
	#[error("track {track}: {status:#04x} is not an event a file may hold")]
	BadStatus { track: usize, status: u8 },
	#[error("track {track}: malformed message {status:#04x}")]
	BadMessage { track: usize, status: u8 },
	#[error("track {track}: a tempo of zero")]
	ZeroTempo { track: usize },
}

/// Reads and parses the file at `path`.
pub fn load(path: &Path) -> Result<Song, SmfError> {
	let bytes = std::fs::read(path).map_err(|source| SmfError::Read {
		path: path.to_owned(),
		source,
	})?;
	parse(&bytes)
}

/// Parses the bytes of a MIDI file.
pub fn parse(bytes: &[u8]) -> Result<Song, SmfError> {
	let mut reader = Reader::new(bytes);
	if reader.take(4, "the header").ok() != Some(HEADER_CHUNK.as_slice()) {
		return Err(SmfError::NotMidi);
	}
	// A header longer than the six bytes defined today is allowed: the rest is skipped.
	let header_length = reader.u32("the header")? as usize;
	if header_length < HEADER_LENGTH {
		return Err(SmfError::Truncated("the header"));
	}
	let mut header = Reader::new(reader.take(header_length, "the header")?);
	let format = header.u16("the header")?;
	let declared_tracks = usize::from(header.u16("the header")?);
	let division = header.u16("the header")?;
	if format > 1 {
		return Err(SmfError::UnsupportedFormat(format));
	}
	if division & 0x8000 != 0 {
		return Err(SmfError::SmpteTiming);
	}
	if division == 0 {
		return Err(SmfError::NoTicks);
	}

	let mut tracks = Vec::with_capacity(declared_tracks.min(64));
	while tracks.len() < declared_tracks {
		let kind = reader.take(4, "a chunk header")?;
		let length = reader.u32("a chunk header")? as usize;
		let data = reader.take(length, "a track")?;
		// Chunks of a kind this reader does not know are to be skipped, the format says.
		if kind == TRACK_CHUNK.as_slice() {
			tracks.push(read_track(data, tracks.len())?);
		}
	}
	Ok(merge(tracks, division))
}

/// An event of one track before the tracks are merged: when, in ticks from the start, and what.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TrackItem {
	Message(MidiEvent),
	/// Microseconds per quarter note from here on.
	Tempo(u32),
}

struct Track {
	items: Vec<(u64, TrackItem)>,
	/// Where the track ends: its End of Track event, or its last event when it has none.
	end_tick: u64,
}

fn read_track(data: &[u8], track: usize) -> Result<Track, SmfError> {
	let mut reader = Reader::new(data);
	let mut items = Vec::new();
	let mut tick = 0u64;
	// The status of the last channel message: a message may leave its status out and reuse this one.
	let mut running_status: Option<u8> = None;
	while !reader.is_empty() {
		tick += u64::from(reader.variable_length(track)?);
		let first = reader.u8("an event")?;
		match first {
			META_EVENT => {
				running_status = None;
				let kind = reader.u8("a meta event")?;
				let length = reader.variable_length(track)? as usize;
				let payload = reader.take(length, "a meta event")?;
				match (kind, payload) {
					(META_END_OF_TRACK, _) => return Ok(Track { items, end_tick: tick }),
					(META_TEMPO, &[high, middle, low]) => {
						let tempo = u32::from_be_bytes([0, high, middle, low]);
						if tempo == 0 {
							return Err(SmfError::ZeroTempo { track });
						}
						items.push((tick, TrackItem::Tempo(tempo)));
					}
					_ => {}
				}
			}
			SYSTEM_EXCLUSIVE | SYSTEM_EXCLUSIVE_CONTINUED => {
				running_status = None;
				let length = reader.variable_length(track)? as usize;
				reader.take(length, "system exclusive data")?;
			}
			status if status >= 0xF0 => return Err(SmfError::BadStatus { track, status }),
			_ => {
				// A data byte in the place of a status: the status of the previous message is meant.
				let (status, first_data) = if first < 0x80 {
					let status = running_status.ok_or(SmfError::NoStatus { track, byte: first })?;
					(status, first)
				} else {
					(first, reader.u8("a message")?)
				};
				running_status = Some(status);
				let has_two_data_bytes = !matches!(status & 0xF0, 0xC0 | 0xD0);
				let message = if has_two_data_bytes {
					parse_message(&[status, first_data, reader.u8("a message")?])
				} else {
					parse_message(&[status, first_data])
				};
				match message.map_err(|_| SmfError::BadMessage { track, status })? {
					// Messages the synthesizer does not act on (polyphonic aftertouch) are left out.
					MidiEvent::Other { .. } => {}
					event => items.push((tick, TrackItem::Message(event))),
				}
			}
		}
	}
	Ok(Track { items, end_tick: tick })
}

/// Merges the tracks in time order and turns ticks into seconds along the tempo changes.
fn merge(tracks: Vec<Track>, division: u16) -> Song {
	let end_tick = tracks.iter().map(|track| track.end_tick).max().unwrap_or(0);
	let mut items: Vec<(u64, TrackItem)> = tracks.into_iter().flat_map(|track| track.items).collect();
	// Stable: what is due at the same tick keeps the order of the tracks and the order within a track.
	items.sort_by_key(|(tick, _)| *tick);

	let mut clock = TempoClock::new(division);
	let mut events = Vec::with_capacity(items.len());
	for (tick, item) in items {
		let seconds = clock.advance_to(tick);
		match item {
			TrackItem::Message(event) => events.push(TimedEvent { seconds, event }),
			TrackItem::Tempo(tempo) => clock.tempo = tempo,
		}
	}
	Song {
		events,
		duration: clock.advance_to(end_tick),
	}
}

/// Walks forward through the file turning ticks into seconds at the tempo in force.
struct TempoClock {
	ticks_per_quarter: f64,
	tempo: u32,
	tick: u64,
	seconds: f64,
}

impl TempoClock {
	fn new(division: u16) -> Self {
		Self {
			ticks_per_quarter: f64::from(division),
			tempo: DEFAULT_TEMPO_MICROSECONDS,
			tick: 0,
			seconds: 0.0,
		}
	}

	/// The time of `tick`, which is not before the tick of the previous call.
	fn advance_to(&mut self, tick: u64) -> f64 {
		let quarters = tick.saturating_sub(self.tick) as f64 / self.ticks_per_quarter;
		self.seconds += quarters * f64::from(self.tempo) / MICROSECONDS_PER_SECOND;
		self.tick = self.tick.max(tick);
		self.seconds
	}
}

/// A cursor over bytes; running out of them names what was being read.
struct Reader<'a> {
	bytes: &'a [u8],
}

impl<'a> Reader<'a> {
	fn new(bytes: &'a [u8]) -> Self {
		Self { bytes }
	}

	fn is_empty(&self) -> bool {
		self.bytes.is_empty()
	}

	fn take(&mut self, count: usize, what: &'static str) -> Result<&'a [u8], SmfError> {
		if count > self.bytes.len() {
			return Err(SmfError::Truncated(what));
		}
		let (taken, rest) = self.bytes.split_at(count);
		self.bytes = rest;
		Ok(taken)
	}

	fn u8(&mut self, what: &'static str) -> Result<u8, SmfError> {
		Ok(self.take(1, what)?[0])
	}

	fn u16(&mut self, what: &'static str) -> Result<u16, SmfError> {
		let bytes = self.take(2, what)?;
		Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
	}

	fn u32(&mut self, what: &'static str) -> Result<u32, SmfError> {
		let bytes = self.take(4, what)?;
		Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
	}

	/// A number of the format's variable length: seven bits a byte, the high bit set on all but the last.
	fn variable_length(&mut self, track: usize) -> Result<u32, SmfError> {
		let mut value = 0u32;
		for _ in 0..VARIABLE_LENGTH_BYTES {
			let byte = self.u8("a number")?;
			value = (value << 7) | u32::from(byte & 0x7F);
			if byte & 0x80 == 0 {
				return Ok(value);
			}
		}
		Err(SmfError::NumberTooLong { track })
	}
}

#[cfg(test)]
mod tests;
