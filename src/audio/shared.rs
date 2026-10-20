//! State shared between the audio callback and other threads through atomics only (spec N-03).

use std::sync::atomic::{AtomicU32, Ordering};

pub struct AudioShared {
	/// Master gain as `f32` bits (0.0..=1.0).
	master_gain_bits: AtomicU32,
	xruns: AtomicU32,
}

impl AudioShared {
	pub fn new(master_gain: f32) -> Self {
		Self {
			master_gain_bits: AtomicU32::new(master_gain.to_bits()),
			xruns: AtomicU32::new(0),
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
	fn xruns_count_up() {
		let shared = AudioShared::default();
		shared.record_xrun();
		shared.record_xrun();
		assert_eq!(shared.xruns(), 2);
	}
}
