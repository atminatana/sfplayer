//! Reverb and chorus for the built-in synthesizer (spec v1.2, F-22): the same two effects the SoundFont
//! engine has inside RustySynth, taken from there, so that both engines answer to the same controls —
//! the send levels CC 91 and CC 93.
//!
//! The synthesizer writes what each voice sends into two buffers, block by block; the effects turn
//! them into what is added to the two sides. Every buffer is allocated at creation (spec N-03).

mod chorus;
mod reverb;

use chorus::Chorus;
use reverb::Reverb;

/// Chorus settings of RustySynth: a delay of 2 ms swaying by 1.9 ms each way, 0.4 times a second.
const CHORUS_DELAY_SECONDS: f64 = 0.002;
const CHORUS_DEPTH_SECONDS: f64 = 0.0019;
const CHORUS_FREQUENCY_HZ: f64 = 0.4;

pub(super) struct Effects {
	reverb: Reverb,
	chorus: Chorus,
	reverb_input: Vec<f32>,
	chorus_input: Vec<f32>,
	wet_left: Vec<f32>,
	wet_right: Vec<f32>,
	/// Frames of the block in progress.
	frames: usize,
}

impl Effects {
	/// The most frames one block may hold.
	pub(super) const BLOCK: usize = 256;
	/// Gain of a full reverb send. A voice sits in the middle, on both sides at once, and both sides
	/// feed the one input of the reverb — as RustySynth has it: its input gain times left plus right.
	pub(super) const REVERB_SEND_GAIN: f32 = 2.0 * reverb::INPUT_GAIN;

	pub(super) fn new(sample_rate: u32) -> Self {
		Self {
			reverb: Reverb::new(sample_rate),
			chorus: Chorus::new(sample_rate, CHORUS_DELAY_SECONDS, CHORUS_DEPTH_SECONDS, CHORUS_FREQUENCY_HZ),
			reverb_input: vec![0.0; Self::BLOCK],
			chorus_input: vec![0.0; Self::BLOCK],
			wet_left: vec![0.0; Self::BLOCK],
			wet_right: vec![0.0; Self::BLOCK],
			frames: 0,
		}
	}

	/// Starts a block of `frames` frames (at most `BLOCK`) and returns its two send buffers, cleared:
	/// what goes to the reverb and what goes to the chorus.
	pub(super) fn begin_block(&mut self, frames: usize) -> (&mut [f32], &mut [f32]) {
		self.frames = frames.min(Self::BLOCK);
		let reverb_input = &mut self.reverb_input[..self.frames];
		let chorus_input = &mut self.chorus_input[..self.frames];
		reverb_input.fill(0.0);
		chorus_input.fill(0.0);
		(reverb_input, chorus_input)
	}

	/// Ends the block: runs both effects over what was sent and adds their output to the two sides.
	pub(super) fn finish_block(&mut self, left: &mut [f32], right: &mut [f32]) {
		let frames = self.frames.min(left.len()).min(right.len());
		let (wet_left, wet_right) = (&mut self.wet_left[..frames], &mut self.wet_right[..frames]);
		let chorus_input = &self.chorus_input[..frames];
		self.chorus.process(chorus_input, chorus_input, wet_left, wet_right);
		add(wet_left, &mut left[..frames]);
		add(wet_right, &mut right[..frames]);
		self.reverb.process(&self.reverb_input[..frames], wet_left, wet_right);
		add(wet_left, &mut left[..frames]);
		add(wet_right, &mut right[..frames]);
	}
}

fn add(source: &[f32], target: &mut [f32]) {
	for (target, source) in target.iter_mut().zip(source) {
		*target += source;
	}
}

#[cfg(test)]
mod tests;
