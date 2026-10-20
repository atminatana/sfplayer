//! Synthesis layer: the `SynthEngine` trait (spec §3) and the SoundFont bank model.
//! Engine choice and isolation rationale: ADR 0001.

mod bank;
mod rusty;

pub use bank::{PresetInfo, SoundFontBank, SynthError};
pub use rusty::RustySynthEngine;

/// A polyphonic synthesizer driven by MIDI-like events and rendering stereo f32 audio.
///
/// Implementations must be allocation-free in every method except construction:
/// `render` runs inside the audio callback (spec N-03).
pub trait SynthEngine: Send {
	fn note_on(&mut self, channel: u8, key: u8, velocity: u8);
	fn note_off(&mut self, channel: u8, key: u8);
	fn control_change(&mut self, channel: u8, controller: u8, value: u8);
	fn program_change(&mut self, channel: u8, program: u8);
	/// `value` is the 14-bit pitch bend (0..=16383, centre 8192).
	fn pitch_bend(&mut self, channel: u8, value: u16);
	/// Renders `left.len()` frames; `left` and `right` must have equal length.
	fn render(&mut self, left: &mut [f32], right: &mut [f32]);
	/// Polyphony limit shown in the status line (F-12); 0 when unknown.
	fn max_voices(&self) -> usize {
		0
	}
}

#[cfg(test)]
pub(crate) mod test_support {
	/// Minimal sine bank shipped with the repository, used by all synth tests.
	pub const TEST_BANK_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/sin_wave.sf2");

	pub fn rms(samples: &[f32]) -> f32 {
		if samples.is_empty() {
			return 0.0;
		}
		let sum_of_squares: f32 = samples.iter().map(|sample| sample * sample).sum();
		(sum_of_squares / samples.len() as f32).sqrt()
	}
}
