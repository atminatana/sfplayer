//! Offline tests of the render core with rustysynth and the repository test bank.

use std::path::Path;

use ringbuf::HeapRb;
use ringbuf::traits::Split;

use super::*;
use crate::synth::test_support::{TEST_BANK_PATH, rms};
use crate::synth::{RustySynthEngine, SoundFontBank};

const SAMPLE_RATE: u32 = 48_000;
const FRAMES: usize = 4_800;
const RING_CAPACITY: usize = 16;
const NOTE_ON: MidiEvent = MidiEvent::NoteOn {
	channel: 0,
	key: 60,
	velocity: 100,
};

struct Harness {
	renderer: BlockRenderer,
	events: HeapProd<MidiEvent>,
	commands: HeapProd<Command>,
	retired: HeapCons<Box<dyn SynthEngine>>,
	recording: HeapCons<f32>,
	shared: Arc<AudioShared>,
}

fn engine() -> Box<dyn SynthEngine> {
	let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
	Box::new(RustySynthEngine::new(&bank, SAMPLE_RATE, true).expect("engine"))
}

/// Renderer with an engine already installed through the command ring.
fn harness() -> Harness {
	let mut harness = empty_harness();
	harness.commands.try_push(Command::ReplaceEngine(Some(engine()))).ok().unwrap();
	harness
}

fn empty_harness() -> Harness {
	let shared = Arc::new(AudioShared::default());
	let (events, events_rx) = HeapRb::<MidiEvent>::new(RING_CAPACITY).split();
	let (commands, commands_rx) = HeapRb::<Command>::new(RING_CAPACITY).split();
	let (retired_tx, retired) = HeapRb::<Box<dyn SynthEngine>>::new(RING_CAPACITY).split();
	let (recording_tx, recording) = HeapRb::<f32>::new(FRAMES * 2 + 16).split();
	let renderer = BlockRenderer::new(events_rx, commands_rx, retired_tx, recording_tx, Arc::clone(&shared), SAMPLE_RATE);
	Harness {
		renderer,
		events,
		commands,
		retired,
		recording,
		shared,
	}
}

fn deinterleave(output: &[f32], channels: usize) -> (Vec<f32>, Vec<f32>) {
	let left = output.iter().step_by(channels).copied().collect();
	let right = output.iter().skip(1).step_by(channels).copied().collect();
	(left, right)
}

#[test]
fn no_engine_means_silence_even_with_events() {
	let mut h = empty_harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![1.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn silence_without_events() {
	let mut h = harness();
	let mut output = vec![1.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn note_from_midi_ring_is_rendered_to_both_stereo_channels() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	let (left, right) = deinterleave(&output, 2);
	assert!(rms(&left) > 0.01, "left {}", rms(&left));
	assert!(rms(&right) > 0.01, "right {}", rms(&right));
}

#[test]
fn note_from_command_ring_takes_the_same_path() {
	let mut h = harness();
	h.commands.try_push(Command::Midi(NOTE_ON)).ok().unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) > 0.01);
}

#[test]
fn replacing_the_engine_returns_the_old_one_through_the_retired_ring() {
	let mut h = harness();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(h.retired.try_pop().is_none());
	h.commands.try_push(Command::ReplaceEngine(None)).ok().unwrap();
	h.events.try_push(NOTE_ON).unwrap();
	h.renderer.fill(&mut output, 2);
	assert!(h.retired.try_pop().is_some(), "old engine handed back");
	assert!(output.iter().all(|sample| *sample == 0.0), "no engine → silence");
}

#[test]
fn levels_are_published_after_each_block() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	let levels = h.shared.levels();
	assert!(levels.peak_left > 0.01 && levels.rms_left > 0.01, "{levels:?}");
	assert!(levels.peak_left >= levels.rms_left);
	h.shared.set_master_gain(0.0);
	h.renderer.fill(&mut output, 2);
	assert_eq!(h.shared.levels(), crate::audio::Levels::default(), "levels follow the master gain");
	assert_eq!(h.shared.clips(), 0);
}

#[test]
fn held_notes_follow_note_on_off_all_notes_off_and_engine_swap() {
	let mut h = harness();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert_eq!(h.shared.max_voices(), 64, "polyphony of the installed engine");
	h.events.try_push(NOTE_ON).unwrap();
	h.events
		.try_push(MidiEvent::NoteOn {
			channel: 1,
			key: 64,
			velocity: 80,
		})
		.unwrap();
	h.events.try_push(NOTE_ON).unwrap();
	h.renderer.fill(&mut output, 2);
	assert_eq!(h.shared.held_notes(), 2, "a repeated note on counts once");
	h.commands
		.try_push(Command::Midi(MidiEvent::NoteOff { channel: 0, key: 60 }))
		.ok()
		.unwrap();
	h.renderer.fill(&mut output, 2);
	assert_eq!(h.shared.held_notes(), 1);
	h.events
		.try_push(MidiEvent::ControlChange {
			channel: 1,
			controller: 123,
			value: 0,
		})
		.unwrap();
	h.renderer.fill(&mut output, 2);
	assert_eq!(h.shared.held_notes(), 0);
	h.events.try_push(NOTE_ON).unwrap();
	h.commands.try_push(Command::ReplaceEngine(None)).ok().unwrap();
	h.renderer.fill(&mut output, 2);
	assert_eq!(
		h.shared.held_notes(),
		1,
		"note on arrives after the swap and is tracked without an engine"
	);
	assert_eq!(h.shared.max_voices(), 0);
}

#[test]
fn transposed_note_off_matches_its_note_on_even_after_the_transpose_changes() {
	let mut h = harness();
	let mut output = vec![0.0; FRAMES * 2];
	h.shared.set_transpose(12);
	h.events.try_push(NOTE_ON).unwrap();
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) > 0.01);
	h.shared.set_transpose(0);
	h.events.try_push(MidiEvent::NoteOff { channel: 0, key: 60 }).unwrap();
	let mut tail = vec![0.0; SAMPLE_RATE as usize * 2];
	h.renderer.fill(&mut tail, 2);
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) < 0.001, "note off reached the transposed key: {}", rms(&output));
}

#[test]
fn omni_redirects_hardware_notes_but_not_ui_notes() {
	let mut h = harness();
	let mut output = vec![0.0; FRAMES * 2];
	h.shared.set_omni_channel(Some(0));
	h.events
		.try_push(MidiEvent::NoteOn {
			channel: 5,
			key: 60,
			velocity: 100,
		})
		.unwrap();
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) > 0.01);
	h.events
		.try_push(MidiEvent::ControlChange {
			channel: 0,
			controller: 123,
			value: 0,
		})
		.unwrap();
	let mut tail = vec![0.0; SAMPLE_RATE as usize * 2];
	h.renderer.fill(&mut tail, 2);
	h.renderer.fill(&mut output, 2);
	assert!(
		rms(&output) < 0.001,
		"the note sounded on channel 0, so all-notes-off there silences it"
	);
	h.commands
		.try_push(Command::Midi(MidiEvent::NoteOn {
			channel: 5,
			key: 60,
			velocity: 100,
		}))
		.ok()
		.unwrap();
	h.renderer.fill(&mut output, 2);
	h.events
		.try_push(MidiEvent::ControlChange {
			channel: 0,
			controller: 123,
			value: 0,
		})
		.unwrap();
	h.renderer.fill(&mut tail, 2);
	h.renderer.fill(&mut output, 2);
	assert!(
		rms(&output) > 0.001,
		"UI notes keep their channel: all-notes-off on channel 0 leaves channel 5 playing"
	);
}

#[test]
fn load_percent_relates_callback_time_to_block_time() {
	assert_eq!(load_percent(0.001, 480, 48_000), 10);
	assert_eq!(load_percent(0.5, 480, 48_000), 999, "clamped");
	assert_eq!(load_percent(0.1, 0, 48_000), 0);
	let mut h = harness();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(h.shared.load_percent() <= 999);
}

#[test]
fn master_gain_scales_output() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	h.shared.set_master_gain(0.0);
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn mono_output_mixes_both_sides() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![0.0; FRAMES];
	h.renderer.fill(&mut output, 1);
	assert!(rms(&output) > 0.01);
}

#[test]
fn extra_channels_are_silent() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![1.0; FRAMES * 4];
	h.renderer.fill(&mut output, 4);
	let third: Vec<f32> = output.iter().skip(2).step_by(4).copied().collect();
	let fourth: Vec<f32> = output.iter().skip(3).step_by(4).copied().collect();
	assert!(third.iter().chain(&fourth).all(|sample| *sample == 0.0));
	let (left, _) = deinterleave(&output, 4);
	assert!(rms(&left) > 0.01);
}

#[test]
fn buffers_larger_than_chunk_are_processed_whole() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let frames = MAX_CHUNK_FRAMES * 2 + 123;
	let mut output = vec![0.0; frames * 2];
	h.renderer.fill(&mut output, 2);
	let tail = &output[output.len() - 200..];
	assert!(rms(tail) > 0.01, "tail of a long buffer must still carry sound");
}

#[test]
fn note_off_event_is_applied() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	h.events.try_push(MidiEvent::NoteOff { channel: 0, key: 60 }).unwrap();
	let mut tail = vec![0.0; SAMPLE_RATE as usize * 2];
	h.renderer.fill(&mut tail, 2);
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) < 0.001, "after note off {}", rms(&output));
}

#[test]
fn recording_ring_gets_the_output_only_while_recording_and_counts_overflow() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert_eq!(h.recording.try_pop(), None, "nothing recorded while off");
	h.shared.set_recording(true);
	h.renderer.fill(&mut output, 2);
	let mut recorded = vec![0.0; FRAMES * 2];
	assert_eq!(h.recording.pop_slice(&mut recorded), FRAMES * 2);
	assert_eq!(recorded, output, "the recording is exactly what went to the device");
	assert_eq!(h.shared.record_dropped(), 0);
	h.renderer.fill(&mut output, 2);
	h.renderer.fill(&mut output, 2);
	assert!(h.shared.record_dropped() > 0, "a full ring is counted, not blocked on");
}

#[test]
fn integer_and_f64_outputs_are_silent_without_an_engine() {
	let mut h = empty_harness();
	let mut i16_out = vec![1i16; 64];
	h.renderer.fill(&mut i16_out, 2);
	assert!(i16_out.iter().all(|sample| *sample == 0));
	let mut i32_out = vec![1i32; 64];
	h.renderer.fill(&mut i32_out, 2);
	assert!(i32_out.iter().all(|sample| *sample == 0));
	let mut i24_out = vec![cpal::I24::new(1).unwrap(); 64];
	h.renderer.fill(&mut i24_out, 2);
	assert!(i24_out.iter().all(|sample| *sample == cpal::I24::new(0).unwrap()));
	let mut f64_out = vec![1.0f64; 64];
	h.renderer.fill(&mut f64_out, 2);
	assert!(f64_out.iter().all(|sample| *sample == 0.0));
}

#[test]
fn interleave_converts_the_f32_render_to_the_output_sample_type() {
	let left = [1.0, -1.0, 0.5, 0.0];
	let right = [0.0, 0.5, -0.5, 0.25];
	let mut i16_out = vec![0i16; 8];
	interleave(&left, &right, 1.0, &mut i16_out, 2);
	assert_eq!(
		i16_out,
		[32_767, 0, -32_768, 16_384, 16_384, -16_384, 0, 8_192],
		"full scale saturates, not wraps"
	);
	let mut i32_out = vec![0i32; 8];
	interleave(&left, &right, 1.0, &mut i32_out, 2);
	assert_eq!(i32_out, [i32::MAX, 0, i32::MIN, 1 << 30, 1 << 30, -(1 << 30), 0, 1 << 29]);
	let mut f64_out = vec![0.0f64; 8];
	interleave(&left, &right, 1.0, &mut f64_out, 2);
	assert_eq!(f64_out, [1.0, 0.0, -1.0, 0.5, 0.5, -0.5, 0.0, 0.25]);
	let mut gained = vec![0i16; 8];
	interleave(&left, &right, 0.5, &mut gained, 2);
	assert_eq!(
		gained,
		[16_384, 0, -16_384, 8_192, 8_192, -8_192, 0, 4_096],
		"gain applies before conversion"
	);
	let mut mono = vec![0i16; 4];
	interleave(&left, &right, 1.0, &mut mono, 1);
	assert_eq!(mono, [16_384, -8_192, 0, 4_096], "mono is the mean of both channels");
	let mut quad = vec![1i16; 8];
	interleave(&left[..2], &right[..2], 1.0, &mut quad, 4);
	assert_eq!(quad, [32_767, 0, 0, 0, -32_768, 16_384, 0, 0], "extra channels are silent");
}

#[test]
fn a_note_reaches_an_i32_output() {
	let mut h = harness();
	h.events.try_push(NOTE_ON).unwrap();
	let mut output = vec![0i32; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(
		output.iter().any(|sample| *sample != 0),
		"the note is audible in the integer stream"
	);
	assert!(h.shared.levels().peak_left > 0.0, "the meter still reads the f32 render");
}
