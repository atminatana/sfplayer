//! Mixer state (spec F-10): master volume with mute, and per-channel CC 7 volume with mute / solo.
//! Pure data; the session turns it into a master-gain atomic and CC 7 messages for the engine.

pub const CHANNELS: usize = 16;
/// MIDI CC 7 default (GM): channels start at this volume.
pub const DEFAULT_CHANNEL_VOLUME: u8 = 100;
pub const MAX_CHANNEL_VOLUME: u8 = 127;
pub const MASTER_STEP: i8 = 1;
pub const MASTER_STEP_FAST: i8 = 5;
pub const CHANNEL_STEP: i8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelStrip {
	/// CC 7 value 0..=127 the user set.
	pub volume: u8,
	pub muted: bool,
	pub solo: bool,
}

impl Default for ChannelStrip {
	fn default() -> Self {
		Self {
			volume: DEFAULT_CHANNEL_VOLUME,
			muted: false,
			solo: false,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mixer {
	/// 0..=100 %.
	pub master: u8,
	pub master_muted: bool,
	pub channels: [ChannelStrip; CHANNELS],
}

impl Mixer {
	pub fn new(master_percent: u8) -> Self {
		Self {
			master: master_percent.min(100),
			master_muted: false,
			channels: [ChannelStrip::default(); CHANNELS],
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

	pub fn adjust_channel(&mut self, channel: usize, delta: i8) {
		if let Some(strip) = self.channels.get_mut(channel) {
			strip.volume = (i16::from(strip.volume) + i16::from(delta)).clamp(0, i16::from(MAX_CHANNEL_VOLUME)) as u8;
		}
	}

	pub fn toggle_mute(&mut self, channel: usize) {
		if let Some(strip) = self.channels.get_mut(channel) {
			strip.muted = !strip.muted;
		}
	}

	pub fn toggle_solo(&mut self, channel: usize) {
		if let Some(strip) = self.channels.get_mut(channel) {
			strip.solo = !strip.solo;
		}
	}

	pub fn any_solo(&self) -> bool {
		self.channels.iter().any(|strip| strip.solo)
	}

	/// CC 7 value the engine should get for `channel`: 0 when muted or when another channel is soloed.
	pub fn effective_volume(&self, channel: usize) -> u8 {
		let Some(strip) = self.channels.get(channel) else {
			return 0;
		};
		if strip.muted || (self.any_solo() && !strip.solo) {
			0
		} else {
			strip.volume
		}
	}

	/// Effective CC 7 for every channel, in order.
	pub fn effective_volumes(&self) -> [u8; CHANNELS] {
		std::array::from_fn(|channel| self.effective_volume(channel))
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

	#[test]
	fn channel_volume_defaults_to_gm_100_and_clamps_to_127() {
		let mut mixer = Mixer::default();
		assert_eq!(mixer.effective_volume(0), DEFAULT_CHANNEL_VOLUME);
		mixer.adjust_channel(0, CHANNEL_STEP * 6);
		assert_eq!(mixer.channels[0].volume, 127);
		mixer.adjust_channel(1, -120);
		assert_eq!(mixer.channels[1].volume, 0);
		mixer.adjust_channel(99, 5);
		assert_eq!(mixer.effective_volume(99), 0, "out of range is silent and ignored");
	}

	#[test]
	fn mute_silences_one_channel_and_solo_silences_the_others() {
		let mut mixer = Mixer::default();
		mixer.toggle_mute(2);
		assert_eq!(mixer.effective_volume(2), 0);
		assert_eq!(mixer.effective_volume(3), 100);
		mixer.toggle_solo(5);
		assert!(mixer.any_solo());
		let volumes = mixer.effective_volumes();
		assert_eq!(volumes[5], 100);
		assert!(volumes.iter().enumerate().all(|(channel, volume)| channel == 5 || *volume == 0));
		mixer.toggle_solo(9);
		assert_eq!(mixer.effective_volume(9), 100, "several solos play together");
		mixer.toggle_mute(5);
		assert_eq!(mixer.effective_volume(5), 0, "mute wins over solo");
		mixer.toggle_solo(5);
		mixer.toggle_solo(9);
		assert!(!mixer.any_solo());
		assert_eq!(mixer.effective_volume(3), 100);
	}
}
