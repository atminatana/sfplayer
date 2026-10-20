//! The real-time render core: runs inside the cpal callback. No allocation, no locking, no I/O here.

use std::sync::Arc;

use ringbuf::HeapCons;
use ringbuf::traits::Consumer;

use super::AudioShared;
use crate::midi::MidiEvent;
use crate::synth::SynthEngine;

/// Frames rendered per inner pass; larger callbacks are processed in chunks of this size.
pub const MAX_CHUNK_FRAMES: usize = 4096;

pub struct BlockRenderer {
	engine: Box<dyn SynthEngine>,
	events: HeapCons<MidiEvent>,
	shared: Arc<AudioShared>,
	left: Vec<f32>,
	right: Vec<f32>,
}

impl BlockRenderer {
	/// Allocates the scratch buffers once; everything after this is allocation-free.
	pub fn new(engine: Box<dyn SynthEngine>, events: HeapCons<MidiEvent>, shared: Arc<AudioShared>) -> Self {
		Self {
			engine,
			events,
			shared,
			left: vec![0.0; MAX_CHUNK_FRAMES],
			right: vec![0.0; MAX_CHUNK_FRAMES],
		}
	}

	/// Fills an interleaved output buffer with `channels` channels per frame.
	/// Channels beyond the first two are silent; a mono output gets the L/R average.
	pub fn fill(&mut self, output: &mut [f32], channels: usize) {
		self.drain_events();
		let gain = self.shared.master_gain();
		for chunk in output.chunks_mut(MAX_CHUNK_FRAMES * channels) {
			let frames = chunk.len() / channels;
			self.engine.render(&mut self.left[..frames], &mut self.right[..frames]);
			interleave(&self.left[..frames], &self.right[..frames], gain, chunk, channels);
		}
	}

	fn drain_events(&mut self) {
		while let Some(event) = self.events.try_pop() {
			apply_event(self.engine.as_mut(), event);
		}
	}
}

fn apply_event(engine: &mut dyn SynthEngine, event: MidiEvent) {
	match event {
		MidiEvent::NoteOn { channel, key, velocity } => engine.note_on(channel, key, velocity),
		MidiEvent::NoteOff { channel, key } => engine.note_off(channel, key),
		MidiEvent::ControlChange {
			channel,
			controller,
			value,
		} => engine.control_change(channel, controller, value),
		MidiEvent::ProgramChange { channel, program } => engine.program_change(channel, program),
		MidiEvent::PitchBend { channel, value } => engine.pitch_bend(channel, value),
		// rustysynth has no channel pressure generator; spec F-07 lists it as handled, so it is consumed here, not logged.
		MidiEvent::ChannelPressure { .. } | MidiEvent::Other { .. } => {}
	}
}

fn interleave(left: &[f32], right: &[f32], gain: f32, output: &mut [f32], channels: usize) {
	for (frame, out) in output.chunks_mut(channels).enumerate() {
		let (l, r) = (left[frame] * gain, right[frame] * gain);
		match out {
			[mono] => *mono = (l + r) * 0.5,
			[first, second, rest @ ..] => {
				*first = l;
				*second = r;
				rest.fill(0.0);
			}
			[] => {}
		}
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use ringbuf::HeapRb;
	use ringbuf::traits::{Producer, Split};

	use super::*;
	use crate::synth::test_support::{TEST_BANK_PATH, rms};
	use crate::synth::{RustySynthEngine, SoundFontBank};

	const SAMPLE_RATE: u32 = 48_000;
	const FRAMES: usize = 4_800;
	const RING_CAPACITY: usize = 16;

	fn renderer(shared: Arc<AudioShared>) -> (BlockRenderer, ringbuf::HeapProd<MidiEvent>) {
		let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
		let engine = RustySynthEngine::new(&bank, SAMPLE_RATE).expect("engine");
		let (producer, consumer) = HeapRb::<MidiEvent>::new(RING_CAPACITY).split();
		(BlockRenderer::new(Box::new(engine), consumer, shared), producer)
	}

	fn deinterleave(output: &[f32], channels: usize) -> (Vec<f32>, Vec<f32>) {
		let left = output.iter().step_by(channels).copied().collect();
		let right = output.iter().skip(1).step_by(channels).copied().collect();
		(left, right)
	}

	#[test]
	fn silence_without_events() {
		let (mut renderer, _producer) = renderer(Arc::new(AudioShared::default()));
		let mut output = vec![1.0; FRAMES * 2];
		renderer.fill(&mut output, 2);
		assert!(output.iter().all(|sample| *sample == 0.0));
	}

	#[test]
	fn note_from_ring_is_rendered_to_both_stereo_channels() {
		let (mut renderer, mut producer) = renderer(Arc::new(AudioShared::default()));
		producer
			.try_push(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			})
			.unwrap();
		let mut output = vec![0.0; FRAMES * 2];
		renderer.fill(&mut output, 2);
		let (left, right) = deinterleave(&output, 2);
		assert!(rms(&left) > 0.01, "left {}", rms(&left));
		assert!(rms(&right) > 0.01, "right {}", rms(&right));
	}

	#[test]
	fn master_gain_scales_output() {
		let shared = Arc::new(AudioShared::default());
		let (mut renderer, mut producer) = renderer(Arc::clone(&shared));
		producer
			.try_push(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			})
			.unwrap();
		shared.set_master_gain(0.0);
		let mut output = vec![0.0; FRAMES * 2];
		renderer.fill(&mut output, 2);
		assert!(output.iter().all(|sample| *sample == 0.0));
	}

	#[test]
	fn mono_output_mixes_both_sides() {
		let (mut renderer, mut producer) = renderer(Arc::new(AudioShared::default()));
		producer
			.try_push(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			})
			.unwrap();
		let mut output = vec![0.0; FRAMES];
		renderer.fill(&mut output, 1);
		assert!(rms(&output) > 0.01);
	}

	#[test]
	fn extra_channels_are_silent() {
		let (mut renderer, mut producer) = renderer(Arc::new(AudioShared::default()));
		producer
			.try_push(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			})
			.unwrap();
		let mut output = vec![1.0; FRAMES * 4];
		renderer.fill(&mut output, 4);
		let third: Vec<f32> = output.iter().skip(2).step_by(4).copied().collect();
		let fourth: Vec<f32> = output.iter().skip(3).step_by(4).copied().collect();
		assert!(third.iter().chain(&fourth).all(|sample| *sample == 0.0));
		let (left, _) = deinterleave(&output, 4);
		assert!(rms(&left) > 0.01);
	}

	#[test]
	fn buffers_larger_than_chunk_are_processed_whole() {
		let (mut renderer, mut producer) = renderer(Arc::new(AudioShared::default()));
		producer
			.try_push(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			})
			.unwrap();
		let frames = MAX_CHUNK_FRAMES * 2 + 123;
		let mut output = vec![0.0; frames * 2];
		renderer.fill(&mut output, 2);
		let tail = &output[output.len() - 200..];
		assert!(rms(tail) > 0.01, "tail of a long buffer must still carry sound");
	}

	#[test]
	fn note_off_event_is_applied() {
		let (mut renderer, mut producer) = renderer(Arc::new(AudioShared::default()));
		producer
			.try_push(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			})
			.unwrap();
		let mut output = vec![0.0; FRAMES * 2];
		renderer.fill(&mut output, 2);
		producer.try_push(MidiEvent::NoteOff { channel: 0, key: 60 }).unwrap();
		let mut tail = vec![0.0; SAMPLE_RATE as usize * 2];
		renderer.fill(&mut tail, 2);
		renderer.fill(&mut output, 2);
		assert!(rms(&output) < 0.001, "after note off {}", rms(&output));
	}
}
