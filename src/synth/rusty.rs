//! `SynthEngine` backed by `rustysynth` (ADR 0001).

use rustysynth::{Synthesizer, SynthesizerSettings};

use super::{SoundFontBank, SynthEngine, SynthError};

const PROGRAM_CHANGE: i32 = 0xC0;
const PITCH_BEND: i32 = 0xE0;

pub struct RustySynthEngine {
	synthesizer: Synthesizer,
}

impl RustySynthEngine {
	/// Allocates all voice and block buffers up front; `sample_rate` must be within 16 000..=192 000 Hz.
	/// `effects` enables rustysynth's reverb and chorus units (their send levels are CC 91 / CC 93).
	pub fn new(bank: &SoundFontBank, sample_rate: u32, effects: bool) -> Result<Self, SynthError> {
		let mut settings = SynthesizerSettings::new(i32::try_from(sample_rate).unwrap_or(i32::MAX));
		settings.enable_reverb_and_chorus = effects;
		let synthesizer = Synthesizer::new(bank.font(), &settings)?;
		Ok(Self { synthesizer })
	}

	pub fn maximum_polyphony(&self) -> usize {
		self.synthesizer.get_maximum_polyphony()
	}
}

impl SynthEngine for RustySynthEngine {
	fn note_on(&mut self, channel: u8, key: u8, velocity: u8) {
		self.synthesizer.note_on(channel.into(), key.into(), velocity.into());
	}

	fn note_off(&mut self, channel: u8, key: u8) {
		self.synthesizer.note_off(channel.into(), key.into());
	}

	fn control_change(&mut self, channel: u8, controller: u8, value: u8) {
		self.synthesizer
			.process_midi_message(channel.into(), 0xB0, controller.into(), value.into());
	}

	fn program_change(&mut self, channel: u8, program: u8) {
		self.synthesizer
			.process_midi_message(channel.into(), PROGRAM_CHANGE, program.into(), 0);
	}

	fn pitch_bend(&mut self, channel: u8, value: u16) {
		let lsb = i32::from(value & 0x7F);
		let msb = i32::from((value >> 7) & 0x7F);
		self.synthesizer.process_midi_message(channel.into(), PITCH_BEND, lsb, msb);
	}

	fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
		self.synthesizer.render(left, right);
	}

	fn max_voices(&self) -> usize {
		self.synthesizer.get_maximum_polyphony()
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use super::*;
	use crate::synth::test_support::{TEST_BANK_PATH, rms};

	const SAMPLE_RATE: u32 = 48_000;
	const FRAMES_100MS: usize = 4_800;
	const MIDDLE_C: u8 = 60;
	const VELOCITY: u8 = 100;

	fn engine() -> RustySynthEngine {
		let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
		RustySynthEngine::new(&bank, SAMPLE_RATE, true).expect("engine created")
	}

	fn render_rms(engine: &mut RustySynthEngine, frames: usize) -> (f32, f32) {
		let mut left = vec![0.0; frames];
		let mut right = vec![0.0; frames];
		engine.render(&mut left, &mut right);
		(rms(&left), rms(&right))
	}

	#[test]
	fn silence_before_any_note() {
		let mut engine = engine();
		let (left, right) = render_rms(&mut engine, FRAMES_100MS);
		assert_eq!(left, 0.0);
		assert_eq!(right, 0.0);
	}

	#[test]
	fn note_on_produces_sound_on_both_channels() {
		let mut engine = engine();
		engine.note_on(0, MIDDLE_C, VELOCITY);
		let (left, right) = render_rms(&mut engine, FRAMES_100MS);
		assert!(left > 0.01, "left rms {left}");
		assert!(right > 0.01, "right rms {right}");
	}

	#[test]
	fn note_off_lets_sound_decay() {
		let mut engine = engine();
		engine.note_on(0, MIDDLE_C, VELOCITY);
		let (sustained, _) = render_rms(&mut engine, FRAMES_100MS);
		engine.note_off(0, MIDDLE_C);
		// Skip the release tail, then measure a window well after it.
		render_rms(&mut engine, SAMPLE_RATE as usize);
		let (released, _) = render_rms(&mut engine, FRAMES_100MS);
		assert!(released < sustained * 0.1, "sustained {sustained}, released {released}");
	}

	#[test]
	fn all_notes_off_controller_silences_channel() {
		let mut engine = engine();
		engine.note_on(0, MIDDLE_C, VELOCITY);
		render_rms(&mut engine, FRAMES_100MS);
		engine.control_change(0, 123, 0);
		render_rms(&mut engine, SAMPLE_RATE as usize);
		let (left, _) = render_rms(&mut engine, FRAMES_100MS);
		assert!(left < 0.001, "rms after all-notes-off {left}");
	}

	#[test]
	fn program_and_pitch_bend_do_not_panic() {
		let mut engine = engine();
		engine.program_change(0, 0);
		engine.pitch_bend(0, 8192);
		engine.pitch_bend(0, 0);
		engine.pitch_bend(0, 16383);
		engine.note_on(0, MIDDLE_C, VELOCITY);
		let (left, _) = render_rms(&mut engine, FRAMES_100MS);
		assert!(left > 0.0);
	}

	#[test]
	fn polyphony_meets_spec_minimum() {
		assert!(engine().maximum_polyphony() >= 64);
	}

	#[test]
	fn unsupported_sample_rate_is_error() {
		let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
		assert!(matches!(RustySynthEngine::new(&bank, 1_000, true), Err(SynthError::Synthesizer(_))));
	}

	#[test]
	fn engine_without_effects_still_renders_and_accepts_send_levels() {
		let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
		let mut dry = RustySynthEngine::new(&bank, SAMPLE_RATE, false).expect("engine created");
		dry.control_change(0, 91, 127);
		dry.control_change(0, 93, 127);
		dry.note_on(0, MIDDLE_C, VELOCITY);
		let (left, _) = render_rms(&mut dry, FRAMES_100MS);
		assert!(left > 0.01, "dry engine rms {left}");
	}
}
