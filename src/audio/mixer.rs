//! Mixer state (spec F-10, master part): master volume with mute. Per-channel strips left v1.1 with the
//! screen redesign (ADR 0010); channel volume follows the engine's CC 7 only.
//! Pure data; the session turns it into a master-gain atomic.

pub const MASTER_STEP: i8 = 1;
pub const MASTER_STEP_FAST: i8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mixer {
	/// 0..=100 %.
	pub master: u8,
	pub master_muted: bool,
}

impl Mixer {
	pub fn new(master_percent: u8) -> Self {
		Self {
			master: master_percent.min(100),
			master_muted: false,
		}
	}

	/// Linear gain the audio thread applies: 0 when muted, else master / 100.
	pub fn master_gain(&self) -> f32 {
		if self.master_muted { 0.0 } else { f32::from(self.master) / 100.0 }
	}

	pub fn adjust_master(&mut self, delta: i8) {
		self.master = (i16::from(self.master) + i16::from(delta)).clamp(0, 100) as u8;
	}

	pub fn toggle_master_mute(&mut self) {
		self.master_muted = !self.master_muted;
	}
}

impl Default for Mixer {
	fn default() -> Self {
		Self::new(80)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn master_steps_are_clamped_and_mute_zeroes_the_gain() {
		let mut mixer = Mixer::new(98);
		mixer.adjust_master(MASTER_STEP_FAST);
		assert_eq!(mixer.master, 100);
		assert_eq!(mixer.master_gain(), 1.0);
		mixer.adjust_master(-MASTER_STEP);
		assert_eq!(mixer.master, 99);
		mixer.toggle_master_mute();
		assert_eq!(mixer.master_gain(), 0.0);
		mixer.toggle_master_mute();
		assert!((mixer.master_gain() - 0.99).abs() < 1e-6);
		let mut quiet = Mixer::new(3);
		quiet.adjust_master(-MASTER_STEP_FAST);
		assert_eq!(quiet.master, 0);
		assert_eq!(Mixer::new(250).master, 100, "constructor clamps");
	}
}
