//! State shared between the audio callback and other threads through atomics only (spec N-03).

use std::sync::atomic::{AtomicI32, AtomicU8, AtomicU32, AtomicU64, Ordering};

use super::meter::Levels;

/// Where a note comes from (spec v1.2, F-21, F-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
	/// The text keyboard and the on-screen piano.
	Keyboard,
	/// The hardware MIDI input.
	MidiIn,
	/// The MIDI file being played.
	File,
}

impl Source {
	pub const ALL: [Self; 3] = [Self::Keyboard, Self::MidiIn, Self::File];
	pub const COUNT: usize = Self::ALL.len();

	/// Whether the omni redirection applies: it is for what comes from outside, not for what is
	/// played in the window, which already goes to the channel of the chosen preset (F-15).
	pub fn follows_omni(self) -> bool {
		self != Self::Keyboard
	}
}

pub struct AudioShared {
	/// Master gain as `f32` bits (0.0..=1.0).
	master_gain_bits: AtomicU32,
	xruns: AtomicU32,
	/// Latest block levels as `f32` bits: peak L, peak R, RMS L, RMS R (spec F-09).
	levels_bits: [AtomicU32; 4],
	/// Total clipped samples; the UI lights CLIP when it changes.
	clips: AtomicU32,
	/// Notes currently held (note on without note off) — the "voices" figure of F-12.
	held_notes: AtomicU32,
	/// Callback time as a percentage of the block duration (F-12).
	load_percent: AtomicU32,
	/// Polyphony limit of the installed engine; 0 without an engine.
	max_voices: AtomicU32,
	/// Semitones added to every note before the engine (F-15), −12..=12.
	transpose: AtomicI32,
	/// Channel (0-based) all hardware MIDI is redirected to; −1 = off (F-15 omni).
	omni_channel: AtomicI32,
	/// What the callback records now (F-20, F-23), a `RecordMode`.
	recording: AtomicU8,
	/// Samples, or events of a MIDI take, the callback could not fit into the recording ring.
	record_dropped: AtomicU32,
	/// Keys sounding now, by source: a bit per key, the low and the high half of the 128 (F-24).
	notes: [[AtomicU64; 2]; Source::COUNT],
}

/// What the audio callback hands to the recorder thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordMode {
	Off = 0,
	/// Its output, as samples (F-20).
	Audio = 1,
	/// The events it feeds the engine, each stamped with its time (F-23).
	Midi = 2,
}

impl AudioShared {
	pub fn new(master_gain: f32) -> Self {
		Self {
			master_gain_bits: AtomicU32::new(master_gain.to_bits()),
			xruns: AtomicU32::new(0),
			levels_bits: [const { AtomicU32::new(0) }; 4],
			clips: AtomicU32::new(0),
			held_notes: AtomicU32::new(0),
			load_percent: AtomicU32::new(0),
			max_voices: AtomicU32::new(0),
			transpose: AtomicI32::new(0),
			omni_channel: AtomicI32::new(-1),
			recording: AtomicU8::new(RecordMode::Off as u8),
			record_dropped: AtomicU32::new(0),
			notes: [const { [const { AtomicU64::new(0) }; 2] }; Source::COUNT],
		}
	}

	pub fn master_gain(&self) -> f32 {
		f32::from_bits(self.master_gain_bits.load(Ordering::Relaxed))
	}

	pub fn set_master_gain(&self, gain: f32) {
		self.master_gain_bits.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
	}

	pub fn xruns(&self) -> u32 {
		self.xruns.load(Ordering::Relaxed)
	}

	pub fn record_xrun(&self) {
		self.xruns.fetch_add(1, Ordering::Relaxed);
	}

	/// Audio thread: publishes the levels of the block just rendered and adds its clipped samples.
	pub fn publish_levels(&self, levels: Levels, clipped: u32) {
		for (slot, value) in self
			.levels_bits
			.iter()
			.zip([levels.peak_left, levels.peak_right, levels.rms_left, levels.rms_right])
		{
			slot.store(value.to_bits(), Ordering::Relaxed);
		}
		if clipped > 0 {
			self.clips.fetch_add(clipped, Ordering::Relaxed);
		}
	}

	pub fn levels(&self) -> Levels {
		let read = |index: usize| f32::from_bits(self.levels_bits[index].load(Ordering::Relaxed));
		Levels {
			peak_left: read(0),
			peak_right: read(1),
			rms_left: read(2),
			rms_right: read(3),
		}
	}

	pub fn clips(&self) -> u32 {
		self.clips.load(Ordering::Relaxed)
	}

	/// Audio thread: per-block statistics for the status line.
	pub fn publish_stats(&self, held_notes: u32, load_percent: u32) {
		self.held_notes.store(held_notes, Ordering::Relaxed);
		self.load_percent.store(load_percent, Ordering::Relaxed);
	}

	pub fn set_max_voices(&self, max_voices: u32) {
		self.max_voices.store(max_voices, Ordering::Relaxed);
	}

	pub fn held_notes(&self) -> u32 {
		self.held_notes.load(Ordering::Relaxed)
	}

	pub fn load_percent(&self) -> u32 {
		self.load_percent.load(Ordering::Relaxed)
	}

	pub fn max_voices(&self) -> u32 {
		self.max_voices.load(Ordering::Relaxed)
	}

	pub fn set_transpose(&self, semitones: i8) {
		self.transpose.store(i32::from(semitones.clamp(-12, 12)), Ordering::Relaxed);
	}

	pub fn transpose(&self) -> i8 {
		self.transpose.load(Ordering::Relaxed) as i8
	}

	pub fn set_omni_channel(&self, channel: Option<u8>) {
		self.omni_channel
			.store(channel.map_or(-1, |channel| i32::from(channel.min(15))), Ordering::Relaxed);
	}

	pub fn omni_channel(&self) -> Option<u8> {
		u8::try_from(self.omni_channel.load(Ordering::Relaxed)).ok()
	}

	pub fn set_recording(&self, mode: RecordMode) {
		self.recording.store(mode as u8, Ordering::Relaxed);
	}

	pub fn recording(&self) -> RecordMode {
		match self.recording.load(Ordering::Relaxed) {
			mode if mode == RecordMode::Audio as u8 => RecordMode::Audio,
			mode if mode == RecordMode::Midi as u8 => RecordMode::Midi,
			_ => RecordMode::Off,
		}
	}

	pub fn record_dropped(&self) -> u32 {
		self.record_dropped.load(Ordering::Relaxed)
	}

	pub fn add_record_dropped(&self, samples: u32) {
		if samples > 0 {
			self.record_dropped.fetch_add(samples, Ordering::Relaxed);
		}
	}

	/// Audio thread: the keys `source` holds now, a bit per key.
	pub fn publish_notes(&self, source: Source, keys: u128) {
		let [low, high] = &self.notes[source as usize];
		low.store(keys as u64, Ordering::Relaxed);
		high.store((keys >> 64) as u64, Ordering::Relaxed);
	}

	/// The keys `source` holds, a bit per key. The halves are read one after the other, so a reader
	/// may for one frame see a note of one half that the other half has not caught up with — it is
	/// a picture, not a protocol.
	pub fn notes(&self, source: Source) -> u128 {
		let [low, high] = &self.notes[source as usize];
		u128::from(low.load(Ordering::Relaxed)) | (u128::from(high.load(Ordering::Relaxed)) << 64)
	}
}

impl Default for AudioShared {
	fn default() -> Self {
		Self::new(1.0)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn gain_round_trips_and_clamps() {
		let shared = AudioShared::default();
		assert_eq!(shared.master_gain(), 1.0);
		shared.set_master_gain(0.25);
		assert_eq!(shared.master_gain(), 0.25);
		shared.set_master_gain(7.0);
		assert_eq!(shared.master_gain(), 1.0);
		shared.set_master_gain(-1.0);
		assert_eq!(shared.master_gain(), 0.0);
	}

	#[test]
	fn levels_round_trip_and_clips_accumulate() {
		let shared = AudioShared::default();
		assert_eq!(shared.levels(), Levels::default());
		let levels = Levels {
			peak_left: 0.9,
			peak_right: 0.1,
			rms_left: 0.5,
			rms_right: 0.05,
		};
		shared.publish_levels(levels, 2);
		shared.publish_levels(levels, 0);
		shared.publish_levels(levels, 3);
		assert_eq!(shared.levels(), levels);
		assert_eq!(shared.clips(), 5);
	}

	#[test]
	fn note_maps_round_trip_across_both_halves_and_stay_apart_by_source() {
		let shared = AudioShared::default();
		for source in Source::ALL {
			assert_eq!(shared.notes(source), 0);
		}
		// Keys 0, 63, 64 and 127: both ends of both halves.
		let keys = 1u128 | (1 << 63) | (1 << 64) | (1 << 127);
		shared.publish_notes(Source::MidiIn, keys);
		assert_eq!(shared.notes(Source::MidiIn), keys);
		assert_eq!(shared.notes(Source::Keyboard), 0);
		shared.publish_notes(Source::Keyboard, 1 << 60);
		shared.publish_notes(Source::MidiIn, 0);
		assert_eq!(shared.notes(Source::Keyboard), 1 << 60);
		assert_eq!(shared.notes(Source::MidiIn), 0);
		assert!(Source::MidiIn.follows_omni() && Source::File.follows_omni() && !Source::Keyboard.follows_omni());
		shared.publish_notes(Source::File, 1 << 72);
		assert_eq!(shared.notes(Source::File), 1 << 72);
		assert_eq!(shared.notes(Source::Keyboard), 1 << 60, "the file has a map of its own");
	}

	#[test]
	fn xruns_count_up() {
		let shared = AudioShared::default();
		shared.record_xrun();
		shared.record_xrun();
		assert_eq!(shared.xruns(), 2);
	}
}
