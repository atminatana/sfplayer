//! Spec N-03: nothing that runs on the audio thread touches the heap.
//!
//! This test binary replaces the global allocator with one that counts, per thread, every allocation
//! and every release. What the audio callback does — events into the engine, rendering, the render core
//! around them — is run between two readings of the counters, and both must stand still.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;

use ringbuf::traits::{Producer, Split};
use ringbuf::{HeapProd, HeapRb};
use sf2_player::audio::{AudioShared, BlockRenderer, Command};
use sf2_player::midi::{MidiEvent, controller};
use sf2_player::synth::{Adsr, BuiltinSynth, MAX_VOICES, RustySynthEngine, SoundFontBank, SynthEngine, Timbre, Wave};

thread_local! {
	/// Allocations and releases made by this thread.
	static HEAP_CALLS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

struct Counting;

fn count(allocations: usize, releases: usize) {
	// A thread that is shutting down has no counter any more; nothing is measured there.
	let _ = HEAP_CALLS.try_with(|calls| {
		let (allocated, released) = calls.get();
		calls.set((allocated + allocations, released + releases));
	});
}

// SAFETY: every call is passed on to the system allocator unchanged; counting touches a thread-local
// `Cell` of plain numbers, which allocates nothing.
unsafe impl GlobalAlloc for Counting {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		count(1, 0);
		// SAFETY: the caller upholds the contract of `GlobalAlloc::alloc`, passed on as is.
		unsafe { System.alloc(layout) }
	}

	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		count(1, 0);
		// SAFETY: as above.
		unsafe { System.alloc_zeroed(layout) }
	}

	unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		count(1, 0);
		// SAFETY: as above.
		unsafe { System.realloc(pointer, layout, new_size) }
	}

	unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
		count(0, 1);
		// SAFETY: as above.
		unsafe { System.dealloc(pointer, layout) }
	}
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// How many times `work` allocated and released memory, on this thread.
fn heap_calls_during(work: impl FnOnce()) -> (usize, usize) {
	let before = HEAP_CALLS.with(Cell::get);
	work();
	let after = HEAP_CALLS.with(Cell::get);
	(after.0 - before.0, after.1 - before.1)
}

const SAMPLE_RATE: u32 = 48_000;
const BLOCK: usize = 256;
const RING: usize = 256;
const TEST_BANK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/sin_wave.sf2");

fn piano() -> Adsr {
	Adsr {
		attack: 0.003,
		decay: 0.5,
		sustain: 0.35,
		release: 0.35,
	}
}

fn note_on(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOn {
		channel,
		key,
		velocity: 100,
	}
}

/// The render core with its four rings; only the ends the test feeds are kept.
struct Core {
	renderer: BlockRenderer,
	events: HeapProd<MidiEvent>,
	file_events: HeapProd<MidiEvent>,
	commands: HeapProd<Command>,
	/// The far ends of the rings the core writes to: kept so that they are not closed.
	_retired: ringbuf::HeapCons<Box<dyn SynthEngine>>,
	_recording: ringbuf::HeapCons<f32>,
	shared: Arc<AudioShared>,
}

fn core_with(engine: Box<dyn SynthEngine>) -> Core {
	let shared = Arc::new(AudioShared::default());
	let (events, events_rx) = HeapRb::<MidiEvent>::new(RING).split();
	let (file_events, file_events_rx) = HeapRb::<MidiEvent>::new(RING).split();
	let (mut commands, commands_rx) = HeapRb::<Command>::new(RING).split();
	let (retired_tx, retired) = HeapRb::<Box<dyn SynthEngine>>::new(4).split();
	let (recording_tx, recording) = HeapRb::<f32>::new(BLOCK * 8).split();
	let mut renderer = BlockRenderer::new(
		events_rx,
		file_events_rx,
		commands_rx,
		retired_tx,
		recording_tx,
		Arc::clone(&shared),
		SAMPLE_RATE,
	);
	commands
		.try_push(Command::ReplaceEngine(Some(engine)))
		.ok()
		.expect("room in the ring");
	// The first callback installs the engine; from then on the core must leave the heap alone.
	renderer.fill(&mut [0.0; BLOCK * 2], 2);
	Core {
		renderer,
		events,
		file_events,
		commands,
		_retired: retired,
		_recording: recording,
		shared,
	}
}

/// What a busy callback sees: notes from all three sources on several channels, controllers, a bend, a
/// panic, recording switched on, and a replaced engine on its way back to the UI thread.
fn busy_callbacks(core: &mut Core, spare_engine: Box<dyn SynthEngine>) {
	let mut output = [0.0f32; BLOCK * 2];
	for key in 30..100 {
		core.events.try_push(note_on(key % 16, key)).expect("room");
		core.file_events.try_push(note_on((key + 3) % 16, key)).expect("room");
		core.commands.try_push(Command::Play(note_on(0, key))).ok().expect("room");
	}
	core.events
		.try_push(MidiEvent::PitchBend { channel: 0, value: 12_000 })
		.expect("room");
	core.events
		.try_push(MidiEvent::ControlChange {
			channel: 0,
			controller: controller::SUSTAIN,
			value: 127,
		})
		.expect("room");
	for channel in 0..16 {
		for controller in [controller::REVERB_SEND, controller::CHORUS_SEND] {
			let message = MidiEvent::ControlChange {
				channel,
				controller,
				value: 100,
			};
			core.commands.try_push(Command::Midi(message)).ok().expect("room");
		}
	}
	core.shared.set_transpose(3);
	core.shared.set_omni_channel(Some(2));
	core.shared.set_recording(true);
	core.renderer.fill(&mut output, 2);
	for key in 30..100 {
		core.events.try_push(MidiEvent::NoteOff { channel: key % 16, key }).expect("room");
	}
	core.renderer.fill(&mut output, 2);
	for channel in 0..16 {
		for controller in [controller::ALL_NOTES_OFF, controller::ALL_SOUND_OFF] {
			let message = MidiEvent::ControlChange {
				channel,
				controller,
				value: 0,
			};
			core.commands.try_push(Command::Midi(message)).ok().expect("room");
		}
	}
	core.renderer.fill(&mut output, 2);
	// The old engine leaves through the retired ring: it is not dropped here.
	core.commands
		.try_push(Command::ReplaceEngine(Some(spare_engine)))
		.ok()
		.expect("room");
	core.events.try_push(note_on(0, 60)).expect("room");
	core.renderer.fill(&mut output, 2);
	core.renderer.fill(&mut [0.0f32; BLOCK], 1);
}

#[test]
fn the_counter_sees_allocations_and_releases() {
	let (allocated, released) = heap_calls_during(|| drop(std::hint::black_box(vec![0u8; 64])));
	assert!(allocated >= 1 && released >= 1, "{allocated} allocations, {released} releases");
	assert_eq!(heap_calls_during(|| ()), (0, 0));
}

#[test]
fn the_builtin_synth_leaves_the_heap_alone_once_created() {
	let timbre = Arc::new(Timbre::new(Wave::Saw, piano()));
	let mut synth = BuiltinSynth::new(SAMPLE_RATE, Arc::clone(&timbre));
	let mut left = [0.0f32; BLOCK];
	let mut right = [0.0f32; BLOCK];
	let calls = heap_calls_during(|| {
		// More notes than voices: the oldest ones are taken over.
		for key in 0..(MAX_VOICES as u8 + 20) {
			synth.note_on(key % 16, key, 100);
		}
		synth.render(&mut left, &mut right);
		timbre.set_wave(Wave::Triangle);
		timbre.set_adsr(Adsr { attack: 0.5, ..piano() });
		synth.note_on(0, 60, 127);
		synth.control_change(0, controller::SUSTAIN, 127);
		synth.control_change(0, controller::VOLUME, 90);
		// Both effects at work on every channel.
		for channel in 0..16 {
			synth.control_change(channel, controller::REVERB_SEND, 100);
			synth.control_change(channel, controller::CHORUS_SEND, 100);
		}
		synth.pitch_bend(0, 16_383);
		synth.program_change(0, 5);
		for key in 0..(MAX_VOICES as u8 + 20) {
			synth.note_off(key % 16, key);
		}
		synth.control_change(0, controller::SUSTAIN, 0);
		synth.control_change(3, controller::ALL_NOTES_OFF, 0);
		synth.control_change(4, controller::ALL_SOUND_OFF, 0);
		for _ in 0..200 {
			synth.render(&mut left, &mut right);
		}
	});
	assert_eq!(calls, (0, 0), "(allocations, releases) on the audio path of the built-in synth");
}

#[test]
fn the_render_core_with_the_builtin_synth_leaves_the_heap_alone() {
	let engine = |wave| Box::new(BuiltinSynth::new(SAMPLE_RATE, Arc::new(Timbre::new(wave, piano())))) as Box<dyn SynthEngine>;
	let mut core = core_with(engine(Wave::Sine));
	let spare = engine(Wave::Triangle);
	let calls = heap_calls_during(|| busy_callbacks(&mut core, spare));
	assert_eq!(calls, (0, 0), "(allocations, releases) in the callback");
}

#[test]
fn the_render_core_with_the_soundfont_engine_leaves_the_heap_alone() {
	let bank = SoundFontBank::load(Path::new(TEST_BANK)).expect("the test bank loads");
	let engine = || Box::new(RustySynthEngine::new(&bank, SAMPLE_RATE, true).expect("engine")) as Box<dyn SynthEngine>;
	let mut core = core_with(engine());
	let spare = engine();
	let calls = heap_calls_during(|| busy_callbacks(&mut core, spare));
	assert_eq!(calls, (0, 0), "(allocations, releases) in the callback");
}
