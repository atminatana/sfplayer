//! Offline tests of the render core with rustysynth and the repository test bank.

use std::path::Path;

use ringbuf::HeapRb;
use ringbuf::traits::Split;

use super::*;
use crate::synth::test_support::{TEST_BANK_PATH, rms};
use crate::synth::{Adsr, BuiltinSynth, RustySynthEngine, SoundFontBank, Timbre, Wave};

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
	file_events: HeapProd<MidiEvent>,
	commands: HeapProd<Command>,
	retired: HeapCons<Box<dyn SynthEngine>>,
	recording: RecordSource,
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
	let (file_events, file_events_rx) = HeapRb::<MidiEvent>::new(RING_CAPACITY).split();
	let (commands, commands_rx) = HeapRb::<Command>::new(RING_CAPACITY).split();
	let (retired_tx, retired) = HeapRb::<Box<dyn SynthEngine>>::new(RING_CAPACITY).split();
	let (recording_tx, recording) = record_rings(FRAMES * 2 + 16, RING_CAPACITY);
	let renderer = BlockRenderer::new(
		events_rx,
		file_events_rx,
		commands_rx,
		retired_tx,
		recording_tx,
		Arc::clone(&shared),
		SAMPLE_RATE,
	);
	Harness {
		renderer,
		events,
		file_events,
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
	h.commands.try_push(Command::Play(NOTE_ON)).ok().unwrap();
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
	h.events.try_push(MidiEvent::NoteOff { channel: 0, key: 60 }).unwrap();
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
		.try_push(Command::Play(MidiEvent::NoteOn {
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
	assert_eq!(h.recording.samples.try_pop(), None, "nothing recorded while off");
	h.shared.set_recording(RecordMode::Audio);
	h.renderer.fill(&mut output, 2);
	let mut recorded = vec![0.0; FRAMES * 2];
	assert_eq!(h.recording.samples.pop_slice(&mut recorded), FRAMES * 2);
	assert_eq!(recorded, output, "the recording is exactly what went to the device");
	assert_eq!(h.shared.record_dropped(), 0);
	h.renderer.fill(&mut output, 2);
	h.renderer.fill(&mut output, 2);
	assert!(h.shared.record_dropped() > 0, "a full ring is counted, not blocked on");
}

fn note_on(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOn {
		channel,
		key,
		velocity: 100,
	}
}

fn note_off(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOff { channel, key }
}

/// The keys of a note map, lowest first.
fn keys(map: u128) -> Vec<u8> {
	(0..128).filter(|key| map & (1u128 << key) != 0).collect()
}

/// One callback's worth of rendering; the note maps are published at its end.
fn render(h: &mut Harness) {
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
}

/// Panic as the UI sends it: All Notes Off and All Sound Off on every channel. The command ring of the
/// harness is shorter than that, so the renderer runs while the ring is fed.
fn panic(h: &mut Harness) {
	for channel in 0..16 {
		for controller in [controller::ALL_NOTES_OFF, controller::ALL_SOUND_OFF] {
			let message = MidiEvent::ControlChange {
				channel,
				controller,
				value: 0,
			};
			h.commands.try_push(Command::Midi(message)).ok().unwrap();
		}
		render(h);
	}
}

#[test]
fn the_notes_of_each_source_are_published_in_its_own_map() {
	let mut h = harness();
	render(&mut h);
	assert_eq!((h.shared.notes(Source::Keyboard), h.shared.notes(Source::MidiIn)), (0, 0));

	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	h.commands.try_push(Command::Play(note_on(0, 64))).ok().unwrap();
	// The map is by key, whatever the channel: the low and the high half of the 128 keys.
	h.events.try_push(note_on(3, 50)).unwrap();
	h.events.try_push(note_on(9, 100)).unwrap();
	render(&mut h);
	assert_eq!(keys(h.shared.notes(Source::Keyboard)), [60, 64]);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [50, 100]);
	assert_eq!(h.shared.held_notes(), 4);

	h.commands.try_push(Command::Play(note_off(0, 60))).ok().unwrap();
	h.events.try_push(note_off(9, 100)).unwrap();
	render(&mut h);
	assert_eq!(keys(h.shared.notes(Source::Keyboard)), [64]);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [50]);
	assert_eq!(h.shared.held_notes(), 2);
}

#[test]
fn the_maps_show_the_key_that_sounds_after_transposition_and_omni() {
	let mut h = harness();
	h.shared.set_transpose(2);
	h.shared.set_omni_channel(Some(0));
	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	h.events.try_push(note_on(7, 60)).unwrap();
	h.events.try_push(note_on(8, 60)).unwrap();
	render(&mut h);
	assert_eq!(keys(h.shared.notes(Source::Keyboard)), [62]);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [62]);
	assert_eq!(
		h.shared.held_notes(),
		1,
		"omni brought both hardware notes to the channel of the window: one note, held three times"
	);
}

#[test]
fn a_note_held_by_two_sources_is_released_by_each_on_its_own() {
	let mut h = harness();
	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	h.events.try_push(note_on(0, 60)).unwrap();
	render(&mut h);
	assert_eq!(keys(h.shared.notes(Source::Keyboard)), [60]);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [60]);
	assert_eq!(h.shared.held_notes(), 1, "the same note on the same channel counts once");

	h.commands.try_push(Command::Play(note_off(0, 60))).ok().unwrap();
	render(&mut h);
	assert_eq!(h.shared.notes(Source::Keyboard), 0, "the window let go");
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [60], "the MIDI input still holds it");
	assert_eq!(h.shared.held_notes(), 1);

	h.events.try_push(note_off(0, 60)).unwrap();
	render(&mut h);
	assert_eq!(h.shared.notes(Source::MidiIn), 0);
	assert_eq!(h.shared.held_notes(), 0);
}

#[test]
fn a_source_releases_the_key_it_started_whatever_another_source_played_since() {
	let mut h = harness();
	// The window plays key 60 an octave up: key 72 sounds.
	h.shared.set_transpose(12);
	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	render(&mut h);
	// The MIDI input plays the same key 60 without transposition: key 60 sounds.
	h.shared.set_transpose(0);
	h.events.try_push(note_on(0, 60)).unwrap();
	render(&mut h);
	assert_eq!(keys(h.shared.notes(Source::Keyboard)), [72]);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [60]);

	// The window lets go of its key 60: that is key 72, not the 60 the MIDI input holds.
	h.commands.try_push(Command::Play(note_off(0, 60))).ok().unwrap();
	render(&mut h);
	assert_eq!(h.shared.notes(Source::Keyboard), 0);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [60]);
	h.events.try_push(note_off(0, 60)).unwrap();
	let mut tail = vec![0.0; SAMPLE_RATE as usize * 2];
	h.renderer.fill(&mut tail, 2);
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) < 0.001, "no key is left sounding: {}", rms(&output));
}

#[test]
fn panic_clears_the_maps_of_every_source() {
	let mut h = harness();
	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	h.events.try_push(note_on(0, 62)).unwrap();
	h.events.try_push(note_on(11, 70)).unwrap();
	render(&mut h);
	assert_eq!(h.shared.held_notes(), 3);
	panic(&mut h);
	assert_eq!((h.shared.notes(Source::Keyboard), h.shared.notes(Source::MidiIn)), (0, 0));
	assert_eq!(h.shared.held_notes(), 0);
}

#[test]
fn silencing_one_channel_clears_that_channel_alone_for_every_source() {
	let mut h = harness();
	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	h.events.try_push(note_on(0, 62)).unwrap();
	h.events.try_push(note_on(1, 64)).unwrap();
	render(&mut h);
	// All Notes Off from the MIDI keyboard on channel 1 (0-based 0).
	h.events
		.try_push(MidiEvent::ControlChange {
			channel: 0,
			controller: controller::ALL_NOTES_OFF,
			value: 0,
		})
		.unwrap();
	render(&mut h);
	assert_eq!(h.shared.notes(Source::Keyboard), 0, "the engine silenced the channel for everyone");
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [64]);
}

#[test]
fn service_messages_of_the_ui_are_nobodys_notes() {
	let mut h = harness();
	h.shared.set_transpose(12);
	h.commands.try_push(Command::Midi(note_on(0, 60))).ok().unwrap();
	h.commands
		.try_push(Command::Midi(MidiEvent::ProgramChange { channel: 0, program: 0 }))
		.ok()
		.unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) > 0.01, "the message reached the engine");
	assert_eq!((h.shared.notes(Source::Keyboard), h.shared.notes(Source::MidiIn)), (0, 0));
	assert_eq!(h.shared.held_notes(), 0);
	// It went as it was, untransposed: releasing key 60 itself silences it.
	h.commands.try_push(Command::Midi(note_off(0, 60))).ok().unwrap();
	let mut tail = vec![0.0; SAMPLE_RATE as usize * 2];
	h.renderer.fill(&mut tail, 2);
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) < 0.001);
}

#[test]
fn replacing_the_engine_drops_the_notes_of_every_source() {
	let mut h = harness();
	h.commands.try_push(Command::Play(note_on(0, 60))).ok().unwrap();
	h.events.try_push(note_on(0, 62)).unwrap();
	render(&mut h);
	h.commands.try_push(Command::ReplaceEngine(Some(engine()))).ok().unwrap();
	render(&mut h);
	assert_eq!((h.shared.notes(Source::Keyboard), h.shared.notes(Source::MidiIn)), (0, 0));
	assert_eq!(h.shared.held_notes(), 0);
}

/// The built-in synthesizer with an envelope that is at full level at once and gone at once.
fn builtin_engine() -> Box<dyn SynthEngine> {
	let organ = Adsr {
		attack: 0.001,
		decay: 0.001,
		sustain: 1.0,
		release: 0.001,
	};
	Box::new(BuiltinSynth::new(SAMPLE_RATE, Arc::new(Timbre::new(Wave::Saw, organ))))
}

#[test]
fn the_processor_is_switched_on_the_fly_between_the_two_engines() {
	let mut h = harness();
	let mut output = vec![0.0; FRAMES * 2];
	// A Sound Font note sounds.
	h.events.try_push(NOTE_ON).unwrap();
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) > 0.01);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [60]);

	// The built-in synthesizer takes over in the same stream: the old engine goes back to the UI
	// thread, the note that sounded is gone with it, and the next one is played by the new engine.
	h.commands.try_push(Command::ReplaceEngine(Some(builtin_engine()))).ok().unwrap();
	h.renderer.fill(&mut output, 2);
	assert!(h.retired.try_pop().is_some(), "the Sound Font engine is handed back");
	assert!(
		output.iter().all(|sample| *sample == 0.0),
		"nothing sounds over from the old engine"
	);
	assert_eq!(h.shared.notes(Source::MidiIn), 0);
	assert_eq!(h.shared.max_voices(), 64);
	h.commands.try_push(Command::Play(note_on(0, 69))).ok().unwrap();
	h.renderer.fill(&mut output, 2);
	let (left, right) = deinterleave(&output, 2);
	assert!(rms(&left) > 0.01 && left == right, "the synthesizer plays, the same on both sides");
	assert_eq!(keys(h.shared.notes(Source::Keyboard)), [69]);

	// And back: the Sound Font plays again.
	h.commands.try_push(Command::ReplaceEngine(Some(engine()))).ok().unwrap();
	h.events.try_push(NOTE_ON).unwrap();
	h.renderer.fill(&mut output, 2);
	assert!(h.retired.try_pop().is_some(), "the synthesizer is handed back");
	assert!(rms(&output) > 0.01);
	assert_eq!(h.shared.notes(Source::Keyboard), 0, "its note did not survive the switch");
}

#[test]
fn the_notes_of_a_midi_file_are_a_source_of_their_own_under_the_common_rules() {
	let mut h = harness();
	// The engine goes in with the first callback; a file message in that same callback would be
	// applied before it and dropped with the engine it was meant for.
	render(&mut h);
	h.shared.set_transpose(2);
	h.shared.set_omni_channel(Some(0));
	h.file_events.try_push(note_on(4, 60)).unwrap();
	h.events.try_push(note_on(0, 50)).unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(rms(&output) > 0.01, "the file is heard");
	// Transposed like any other note, and counted for the file alone.
	assert_eq!(keys(h.shared.notes(Source::File)), [62]);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [52]);
	assert_eq!(h.shared.notes(Source::Keyboard), 0);

	// Omni brought the file's note to channel 1: silencing that channel ends it.
	h.events
		.try_push(MidiEvent::ControlChange {
			channel: 0,
			controller: controller::ALL_NOTES_OFF,
			value: 0,
		})
		.unwrap();
	render(&mut h);
	assert_eq!(h.shared.notes(Source::File), 0);

	// The file lets go of its own note without touching the same key held from the MIDI input.
	h.shared.set_omni_channel(None);
	h.file_events.try_push(note_on(0, 60)).unwrap();
	h.events.try_push(note_on(0, 60)).unwrap();
	render(&mut h);
	h.file_events.try_push(note_off(0, 60)).unwrap();
	render(&mut h);
	assert_eq!(h.shared.notes(Source::File), 0);
	assert_eq!(keys(h.shared.notes(Source::MidiIn)), [62]);
}

#[test]
fn what_the_file_sent_is_applied_before_what_the_ui_sends_in_the_same_callback() {
	let mut h = harness();
	// The last of a song and, right behind it, the UI putting the synthesizer in order: both are in
	// the rings when the callback runs. The file's note must be on before the UI silences the channel —
	// the other way round the note would be left sounding.
	h.file_events.try_push(note_on(0, 60)).unwrap();
	h.commands
		.try_push(Command::Midi(MidiEvent::ControlChange {
			channel: 0,
			controller: controller::ALL_SOUND_OFF,
			value: 0,
		}))
		.ok()
		.unwrap();
	let mut output = vec![0.0; FRAMES * 2];
	h.renderer.fill(&mut output, 2);
	assert!(
		output.iter().all(|sample| *sample == 0.0),
		"the note was cut by the message sent after it"
	);
	assert_eq!(h.shared.notes(Source::File), 0);
}

/// What the renderer has put into the ring of a MIDI take.
fn recorded_events(h: &mut Harness) -> Vec<StampedEvent> {
	std::iter::from_fn(|| h.recording.events.try_pop()).collect()
}

fn stamped(frame: usize, event: MidiEvent) -> StampedEvent {
	StampedEvent {
		frame: frame as u64,
		event,
	}
}

#[test]
fn a_midi_take_gets_what_the_engine_is_fed_stamped_with_the_frame_of_the_take() {
	let mut h = harness();
	h.events.try_push(note_on(0, 60)).unwrap();
	render(&mut h);
	assert!(recorded_events(&mut h).is_empty(), "nothing is recorded before the take");

	h.shared.set_recording(RecordMode::Midi);
	h.shared.set_transpose(2);
	h.shared.set_omni_channel(Some(5));
	// One note from each source and a service message of the UI, all in the first callback of the take.
	let silence_channel = MidiEvent::ControlChange {
		channel: 3,
		controller: controller::ALL_NOTES_OFF,
		value: 0,
	};
	h.events.try_push(note_on(0, 70)).unwrap();
	h.commands.try_push(Command::Play(note_on(0, 50))).ok().unwrap();
	h.commands.try_push(Command::Midi(silence_channel)).ok().unwrap();
	h.file_events.try_push(note_on(1, 40)).unwrap();
	render(&mut h);
	h.events.try_push(note_off(0, 70)).unwrap();
	render(&mut h);
	render(&mut h);
	let bend = MidiEvent::PitchBend { channel: 4, value: 9_000 };
	h.events.try_push(bend).unwrap();
	render(&mut h);
	assert_eq!(
		recorded_events(&mut h),
		[
			// In the order the engine got them: the file, the window, the MIDI input. Keys are the
			// ones that sound (transposed); what comes from outside is on the omni channel.
			stamped(0, note_on(5, 42)),
			stamped(0, note_on(0, 52)),
			stamped(0, silence_channel),
			stamped(0, note_on(5, 72)),
			stamped(FRAMES, note_off(5, 72)),
			stamped(FRAMES * 3, bend),
		]
	);
	assert_eq!(h.recording.samples.try_pop(), None, "a MIDI take records no sound");
	assert_eq!(h.shared.record_dropped(), 0);

	// Another take counts its frames from its own start.
	h.shared.set_recording(RecordMode::Off);
	h.events.try_push(note_on(0, 61)).unwrap();
	render(&mut h);
	assert!(recorded_events(&mut h).is_empty(), "nothing is recorded between takes");
	h.shared.set_recording(RecordMode::Midi);
	render(&mut h);
	h.events.try_push(bend).unwrap();
	render(&mut h);
	assert_eq!(recorded_events(&mut h), [stamped(FRAMES, bend)]);
}

#[test]
fn an_audio_take_records_no_events_and_a_full_event_ring_is_counted() {
	let mut h = harness();
	h.shared.set_recording(RecordMode::Audio);
	h.events.try_push(note_on(0, 60)).unwrap();
	render(&mut h);
	assert!(
		recorded_events(&mut h).is_empty(),
		"an audio take records the sound, not the events"
	);
	assert!(h.recording.samples.try_pop().is_some());

	// Nobody reads the ring of the MIDI take: what does not fit is counted, the callback goes on.
	h.shared.set_recording(RecordMode::Midi);
	let before = h.shared.record_dropped();
	for key in 0..RING_CAPACITY as u8 {
		h.events.try_push(note_on(0, key)).unwrap();
	}
	render(&mut h);
	assert_eq!(h.shared.record_dropped(), before, "the ring holds this many");
	for key in 0..5 {
		h.events.try_push(note_off(0, key)).unwrap();
	}
	render(&mut h);
	assert_eq!(h.shared.record_dropped(), before + 5);
	assert_eq!(recorded_events(&mut h).len(), RING_CAPACITY);
}

#[test]
fn the_record_mode_is_published_as_it_was_set() {
	let shared = AudioShared::default();
	assert_eq!(shared.recording(), RecordMode::Off);
	for mode in [RecordMode::Audio, RecordMode::Midi, RecordMode::Off] {
		shared.set_recording(mode);
		assert_eq!(shared.recording(), mode);
	}
}
