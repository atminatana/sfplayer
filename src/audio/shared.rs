//! State shared between the audio callback and other threads through atomics only (spec N-03).

use std::sync::atomic::{AtomicU32, Ordering};

use super::meter::Levels;

pub struct AudioShared {
	/// Master gain as `f32` bits (0.0..=1.0).
	master_gain_bits: AtomicU32,
	xruns: AtomicU32,
	/// Latest block levels as `f32` bits: peak L, peak R, RMS L, RMS R (spec F-09).
	levels_bits: [AtomicU32; 4],
	/// Total clipped samples; the UI lights CLIP when it changes.
	clips: AtomicU32,
}

impl AudioShared {
	pub fn new(master_gain: f32) -> Self {
		Self {
			master_gain_bits: AtomicU32::new(master_gain.to_bits()),
			xruns: AtomicU32::new(0),
			levels_bits: [const { AtomicU32::new(0) }; 4],
			clips: AtomicU32::new(0),
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
