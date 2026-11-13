//! A take of MIDI events (spec v1.2, F-23, ADR 0015): what the audio thread fed the synthesizer, written
//! as a Standard MIDI File of format 0 while the take runs.
//!
//! The events reach the disk as they come, so the file grows like an audio take does and the length of
//! its track is filled in at the end. Notes still sounding when the take ends are ended in the file:
//! a file that leaves notes on would hang them in whatever plays it.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::RecordError;
use crate::audio::StampedEvent;
use crate::midi::MidiEvent;
use crate::midi::smf::write::{TICKS_PER_SECOND, TRACK_LENGTH_OFFSET, file_head, push_end_of_track, push_message, push_tempo, tick_of};

const MIDI_CHANNELS: usize = 16;

pub struct MidiTake {
	path: PathBuf,
	file: BufWriter<File>,
	sample_rate: u32,
	/// Tick of the last event written.
	tick: u64,
	/// Bytes of the track written so far.
	track_length: u32,
	/// Notes begun and not ended, a bit per key for each channel.
	sounding: [u128; MIDI_CHANNELS],
	started: Instant,
	/// One event on its way to the file.
	scratch: Vec<u8>,
}

impl MidiTake {
	pub fn create(path: &Path, sample_rate: u32) -> Result<Self, RecordError> {
		let file = File::create(path).map_err(|source| RecordError::Create {
			path: path.to_owned(),
			source,
		})?;
		let mut take = Self {
			path: path.to_owned(),
			file: BufWriter::new(file),
			sample_rate,
			tick: 0,
			track_length: 0,
			sounding: [0; MIDI_CHANNELS],
			started: Instant::now(),
			scratch: Vec::with_capacity(8),
		};
		// The length of the track is not known yet: zero stands in its place until the take ends.
		take.put(&file_head(0))?;
		push_tempo(&mut take.scratch);
		take.put_event()?;
		Ok(take)
	}

	/// One event the synthesizer was fed, stamped with the audio frame of the take it fell on.
	pub fn write(&mut self, stamped: StampedEvent) -> Result<(), RecordError> {
		let tick = tick_of(stamped.frame, self.sample_rate).max(self.tick);
		if !push_message(&mut self.scratch, wait(self.tick, tick), stamped.event) {
			return Ok(());
		}
		self.tick = tick;
		match stamped.event {
			MidiEvent::NoteOn { channel, key, velocity } if velocity > 0 => self.mark(channel, key, true),
			MidiEvent::NoteOn { channel, key, .. } | MidiEvent::NoteOff { channel, key } => self.mark(channel, key, false),
			_ => {}
		}
		self.put_event()
	}

	/// Hands what was written so far to the system: the file on the disk grows with the take.
	pub fn flush(&mut self) -> Result<(), RecordError> {
		self.file.flush().map_err(|source| self.write_error(source))
	}

	/// Ends the file where the take ended: as long after its start as the take ran.
	pub fn finish(self) -> Result<(), RecordError> {
		let ran = self.started.elapsed().as_secs_f64();
		self.finish_at((ran * TICKS_PER_SECOND as f64) as u64)
	}

	/// Ends the notes still sounding and the track at `end_tick` (not before the last event), then
	/// writes the length of the track into its head.
	fn finish_at(mut self, end_tick: u64) -> Result<(), RecordError> {
		let end_tick = end_tick.max(self.tick);
		for channel in 0..MIDI_CHANNELS {
			for key in 0..128u8 {
				if self.sounding[channel] & (1u128 << key) == 0 {
					continue;
				}
				let note_off = MidiEvent::NoteOff {
					channel: channel as u8,
					key,
				};
				push_message(&mut self.scratch, wait(self.tick, end_tick), note_off);
				self.tick = end_tick;
				self.put_event()?;
			}
		}
		push_end_of_track(&mut self.scratch, wait(self.tick, end_tick));
		self.put_event()?;
		self.file.flush().map_err(|source| self.write_error(source))?;
		self.file
			.seek(SeekFrom::Start(TRACK_LENGTH_OFFSET))
			.map_err(|source| self.write_error(source))?;
		let length = self.track_length.to_be_bytes();
		self.put(&length)?;
		self.file.flush().map_err(|source| self.write_error(source))
	}

	fn mark(&mut self, channel: u8, key: u8, is_sounding: bool) {
		let Some(keys) = self.sounding.get_mut(usize::from(channel)) else {
			return;
		};
		let bit = 1u128 << (key & 0x7F);
		if is_sounding {
			*keys |= bit;
		} else {
			*keys &= !bit;
		}
	}

	/// Writes the event waiting in the scratch buffer and counts it into the track.
	fn put_event(&mut self) -> Result<(), RecordError> {
		let event = std::mem::take(&mut self.scratch);
		self.track_length = self.track_length.saturating_add(event.len() as u32);
		let written = self.put(&event);
		self.scratch = event;
		self.scratch.clear();
		written
	}

	fn put(&mut self, bytes: &[u8]) -> Result<(), RecordError> {
		self.file.write_all(bytes).map_err(|source| self.write_error(source))
	}

	fn write_error(&self, source: std::io::Error) -> RecordError {
		RecordError::Write {
			path: self.path.clone(),
			source,
		}
	}
}

/// Ticks from `from` to `to`, as much of it as one wait of the format can hold.
fn wait(from: u64, to: u64) -> u32 {
	u32::try_from(to.saturating_sub(from)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
