//! Reverb of the built-in synthesizer: the Freeverb design — eight comb filters in parallel and four
//! all-pass filters in series for each side, the right side tuned a little longer than the left.
//!
//! Ported from RustySynth 1.3.8, `src/reverb.rs` (<https://github.com/sinshu/rustysynth>),
//! Copyright (C) 2022 Nobuaki Tanaka, MIT License — the full notice is in `LICENSE-RUSTYSYNTH` at the
//! root of the repository. The filters, their tunings and their coefficients are unchanged. Left out
//! is what RustySynth never uses: changing the room, the damping, the wet level and the width after
//! creation, and the cross-mix of the two sides, which at the fixed full width passes each side
//! through as it is.

/// Gain applied to what is sent into the reverb.
pub(super) const INPUT_GAIN: f32 = 0.015;

const SCALE_DAMP: f32 = 0.4;
const SCALE_ROOM: f32 = 0.28;
const OFFSET_ROOM: f32 = 0.7;
const INITIAL_ROOM: f32 = 0.5;
const INITIAL_DAMP: f32 = 0.5;
/// Feedback of the comb filters: the size of the room.
const ROOM_SIZE: f32 = INITIAL_ROOM * SCALE_ROOM + OFFSET_ROOM;
/// How much of the high end the comb filters lose on every pass.
const DAMP: f32 = INITIAL_DAMP * SCALE_DAMP;
const ALL_PASS_FEEDBACK: f32 = 0.5;
/// Samples the right side's delay lines are longer by, at the reference rate.
const STEREO_SPREAD: usize = 23;
/// Delay line lengths in samples at the reference rate.
const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALL_PASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
const REFERENCE_RATE: f64 = 44_100.0;
/// Values smaller than this are taken for zero, so that a dying tail does not sink into denormal
/// numbers, which are slow to compute with.
const SILENCE: f32 = 1.0e-6;

pub(super) struct Reverb {
	combs_left: Vec<CombFilter>,
	combs_right: Vec<CombFilter>,
	all_passes_left: Vec<AllPassFilter>,
	all_passes_right: Vec<AllPassFilter>,
}

impl Reverb {
	/// Allocates every delay line; nothing is allocated afterwards.
	pub(super) fn new(sample_rate: u32) -> Self {
		let combs = |spread: usize| {
			COMB_TUNINGS
				.iter()
				.map(|tuning| CombFilter::new(scale_tuning(sample_rate, tuning + spread)))
				.collect()
		};
		let all_passes = |spread: usize| {
			ALL_PASS_TUNINGS
				.iter()
				.map(|tuning| AllPassFilter::new(scale_tuning(sample_rate, tuning + spread)))
				.collect()
		};
		Self {
			combs_left: combs(0),
			combs_right: combs(STEREO_SPREAD),
			all_passes_left: all_passes(0),
			all_passes_right: all_passes(STEREO_SPREAD),
		}
	}

	/// Replaces the two outputs with the reverberation of `input`; the three have the same length.
	pub(super) fn process(&mut self, input: &[f32], output_left: &mut [f32], output_right: &mut [f32]) {
		output_left.fill(0.0);
		output_right.fill(0.0);
		for comb in &mut self.combs_left {
			comb.process(input, output_left);
		}
		for all_pass in &mut self.all_passes_left {
			all_pass.process(output_left);
		}
		for comb in &mut self.combs_right {
			comb.process(input, output_right);
		}
		for all_pass in &mut self.all_passes_right {
			all_pass.process(output_right);
		}
	}
}

/// A tuning given for the reference rate, in samples at `sample_rate`; never an empty delay line.
fn scale_tuning(sample_rate: u32, tuning: usize) -> usize {
	let scaled = (f64::from(sample_rate) / REFERENCE_RATE * tuning as f64).round() as usize;
	scaled.max(1)
}

/// A delay line fed back through a one-pole low-pass filter.
struct CombFilter {
	buffer: Vec<f32>,
	buffer_index: usize,
	filter_store: f32,
}

impl CombFilter {
	fn new(buffer_size: usize) -> Self {
		Self {
			buffer: vec![0.0; buffer_size],
			buffer_index: 0,
			filter_store: 0.0,
		}
	}

	/// Adds the output of the filter for `input_block` to `output_block`.
	fn process(&mut self, input_block: &[f32], output_block: &mut [f32]) {
		let buffer_length = self.buffer.len();
		let mut block_index = 0;
		while block_index < output_block.len() {
			if self.buffer_index == buffer_length {
				self.buffer_index = 0;
			}
			let run = (buffer_length - self.buffer_index).min(output_block.len() - block_index);
			for offset in 0..run {
				let block_position = block_index + offset;
				let buffer_position = self.buffer_index + offset;
				let mut output = self.buffer[buffer_position];
				if output.abs() < SILENCE {
					output = 0.0;
				}
				self.filter_store = output * (1.0 - DAMP) + self.filter_store * DAMP;
				if self.filter_store.abs() < SILENCE {
					self.filter_store = 0.0;
				}
				self.buffer[buffer_position] = input_block[block_position] + self.filter_store * ROOM_SIZE;
				output_block[block_position] += output;
			}
			self.buffer_index += run;
			block_index += run;
		}
	}
}

/// A delay line that smears the echoes of the comb filters without colouring the sound.
struct AllPassFilter {
	buffer: Vec<f32>,
	buffer_index: usize,
}

impl AllPassFilter {
	fn new(buffer_size: usize) -> Self {
		Self {
			buffer: vec![0.0; buffer_size],
			buffer_index: 0,
		}
	}

	/// Filters `block` in place.
	fn process(&mut self, block: &mut [f32]) {
		let buffer_length = self.buffer.len();
		let mut block_index = 0;
		while block_index < block.len() {
			if self.buffer_index == buffer_length {
				self.buffer_index = 0;
			}
			let run = (buffer_length - self.buffer_index).min(block.len() - block_index);
			for offset in 0..run {
				let block_position = block_index + offset;
				let buffer_position = self.buffer_index + offset;
				let input = block[block_position];
				let mut delayed = self.buffer[buffer_position];
				if delayed.abs() < SILENCE {
					delayed = 0.0;
				}
				block[block_position] = delayed - input;
				self.buffer[buffer_position] = input + delayed * ALL_PASS_FEEDBACK;
			}
			self.buffer_index += run;
			block_index += run;
		}
	}
}
