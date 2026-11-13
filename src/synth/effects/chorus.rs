//! Chorus of the built-in synthesizer: each side is a copy of its input delayed by a time that sways
//! slowly, the two sides a quarter of the sway apart.
//!
//! Ported from RustySynth 1.3.8, `src/chorus.rs` (<https://github.com/sinshu/rustysynth>),
//! Copyright (C) 2022 Nobuaki Tanaka, MIT License — the full notice is in `LICENSE-RUSTYSYNTH` at the
//! root of the repository. The algorithm is unchanged; only `mute`, which this program does not call,
//! is left out.

use std::f64::consts::TAU;

pub(super) struct Chorus {
	buffer_left: Vec<f32>,
	buffer_right: Vec<f32>,
	/// The delay in samples for every step of one sway.
	delay_table: Vec<f32>,
	buffer_index: usize,
	delay_table_index_left: usize,
	delay_table_index_right: usize,
}

impl Chorus {
	/// `delay` and `depth` in seconds: the delay sways between `delay - depth` and `delay + depth`,
	/// `frequency` times a second. Allocates the delay lines and the table; nothing afterwards.
	pub(super) fn new(sample_rate: u32, delay: f64, depth: f64, frequency: f64) -> Self {
		let rate = f64::from(sample_rate);
		let buffer_length = (rate * (delay + depth)) as usize + 2;
		let table_length = ((rate / frequency).round() as usize).max(1);
		let delay_table = (0..table_length)
			.map(|step| {
				let phase = TAU * step as f64 / table_length as f64;
				(rate * (delay + depth * phase.sin())) as f32
			})
			.collect();
		Self {
			buffer_left: vec![0.0; buffer_length],
			buffer_right: vec![0.0; buffer_length],
			delay_table,
			buffer_index: 0,
			delay_table_index_left: 0,
			delay_table_index_right: table_length / 4,
		}
	}

	/// Replaces the two outputs with the delayed copies of the two inputs; all four have the same length.
	pub(super) fn process(&mut self, input_left: &[f32], input_right: &[f32], output_left: &mut [f32], output_right: &mut [f32]) {
		let buffer_length = self.buffer_left.len();
		let table_length = self.delay_table.len();
		for frame in 0..output_left.len() {
			output_left[frame] = delayed(&self.buffer_left, self.buffer_index, self.delay_table[self.delay_table_index_left]);
			self.delay_table_index_left += 1;
			if self.delay_table_index_left == table_length {
				self.delay_table_index_left = 0;
			}

			output_right[frame] = delayed(
				&self.buffer_right,
				self.buffer_index,
				self.delay_table[self.delay_table_index_right],
			);
			self.delay_table_index_right += 1;
			if self.delay_table_index_right == table_length {
				self.delay_table_index_right = 0;
			}

			self.buffer_left[self.buffer_index] = input_left[frame];
			self.buffer_right[self.buffer_index] = input_right[frame];
			self.buffer_index += 1;
			if self.buffer_index == buffer_length {
				self.buffer_index = 0;
			}
		}
	}
}

/// What was written `delay` samples before `write_index`, between two samples of the delay line when
/// the delay is not a whole number.
fn delayed(buffer: &[f32], write_index: usize, delay: f32) -> f32 {
	let mut position = write_index as f64 - f64::from(delay);
	if position < 0.0 {
		position += buffer.len() as f64;
	}
	let first = position as usize;
	let mut second = first + 1;
	if second == buffer.len() {
		second = 0;
	}
	let before = f64::from(buffer[first]);
	let after = f64::from(buffer[second]);
	let share = position - first as f64;
	(before + share * (after - before)) as f32
}
