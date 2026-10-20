//! State shared between the audio callback and other threads through atomics only (spec N-03).

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use super::meter::Levels;

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
	fn xruns_count_up() {
		let shared = AudioShared::default();
		shared.record_xrun();
		shared.record_xrun();
		assert_eq!(shared.xruns(), 2);
	}
}
