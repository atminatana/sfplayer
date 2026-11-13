//! The real-time render core: runs inside the cpal callback. No allocation, no locking, no I/O here.
//!
//! Three inbound rings feed the renderer: MIDI events from the MIDI thread, the messages of a MIDI
//! file from the thread that plays it, and `Command`s from the UI thread (ADR 0002, 0009, 0014).
//! Replaced engines travel back to the UI thread through the `retired` ring so that deallocation
//! never happens on the audio thread.
//!
//! Every note has a source (the window, the MIDI input, the file): which keys are held and what key a played
//! one sounds as are kept per source, so two sources playing the same note do not undo each other,
//! and the screen can show whose notes sound (spec v1.2, F-24).

use std::sync::Arc;
use std::time::Instant;

use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::meter::measure;
use super::{AudioShared, RecordMode, Source};
use crate::midi::{MidiEvent, controller};
use crate::synth::SynthEngine;

/// Frames rendered per inner pass; larger callbacks are processed in chunks of this size.
pub const MAX_CHUNK_FRAMES: usize = 4096;

const MIDI_CHANNELS: usize = 16;
const KEYS: usize = 128;

/// Instructions from the UI thread to the audio thread.
pub enum Command {
	/// A note played in the window (text keyboard, on-screen piano): transposed and tracked like a
	/// hardware note, as the notes of `Source::Keyboard`.
	Play(MidiEvent),
	/// A service message of the UI (panic, preset, effect sends): reaches the engine as it is and is
	/// nobody's note.
	Midi(MidiEvent),
	/// Swap the synthesizer; `None` silences the output. The previous engine is returned via the retired ring.
	ReplaceEngine(Option<Box<dyn SynthEngine>>),
}

/// An event the engine was fed during a MIDI take, and the audio frame of the take it fell on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StampedEvent {
	pub frame: u64,
	pub event: MidiEvent,
}

/// The audio thread's ends of the two rings a take is recorded through.
pub struct RecordSink {
	/// Interleaved stereo f32: the output, while an audio take runs.
	pub samples: HeapProd<f32>,
	/// What the engine is fed, while a MIDI take runs.
	pub events: HeapProd<StampedEvent>,
}

/// The recorder's ends of the same rings. They travel together: the session lends them to a take
/// and gets them back when it ends.
pub struct RecordSource {
	pub samples: HeapCons<f32>,
	pub events: HeapCons<StampedEvent>,
}

/// The two recording rings, for `samples` samples and `events` events.
pub fn record_rings(samples: usize, events: usize) -> (RecordSink, RecordSource) {
	let (samples_tx, samples_rx) = HeapRb::<f32>::new(samples).split();
	let (events_tx, events_rx) = HeapRb::<StampedEvent>::new(events).split();
	(
		RecordSink {
			samples: samples_tx,
			events: events_tx,
		},
		RecordSource {
			samples: samples_rx,
			events: events_rx,
		},
	)
}

pub struct BlockRenderer {
	engine: Option<Box<dyn SynthEngine>>,
	events: HeapCons<MidiEvent>,
	/// The messages of the MIDI file being played, from its player thread.
	file_events: HeapCons<MidiEvent>,
	commands: HeapCons<Command>,
	retired: HeapProd<Box<dyn SynthEngine>>,
	/// audio → recorder thread: the output or the events of a take, by what is being recorded.
	recording: RecordSink,
	/// What this callback records; read once a callback, so a take changes between callbacks only.
	record_mode: RecordMode,
	/// Frames rendered since the MIDI take began: the time stamp of its events.
	take_frames: u64,
	shared: Arc<AudioShared>,
	sample_rate: u32,
	left: Vec<f32>,
	right: Vec<f32>,
	/// Interleaved copy of a chunk for the recording ring.
	record_scratch: Vec<f32>,
	/// Bit per sounding key, per channel, per source: which notes each source holds.
	held: [[u128; MIDI_CHANNELS]; Source::COUNT],
	/// Key actually sounding for each (source, channel, played key), so a note off matches its note on
	/// even if the transposition changed in between (F-15) or another source played the same key.
	sounding: [[[u8; KEYS]; MIDI_CHANNELS]; Source::COUNT],
}

impl BlockRenderer {
	/// Allocates the scratch buffers once; everything after this is allocation-free.
	pub fn new(
		events: HeapCons<MidiEvent>,
		file_events: HeapCons<MidiEvent>,
		commands: HeapCons<Command>,
		retired: HeapProd<Box<dyn SynthEngine>>,
		recording: RecordSink,
		shared: Arc<AudioShared>,
		sample_rate: u32,
	) -> Self {
		Self {
			engine: None,
			events,
			file_events,
			commands,
			retired,
			recording,
			record_mode: RecordMode::Off,
			take_frames: 0,
			shared,
			sample_rate,
			left: vec![0.0; MAX_CHUNK_FRAMES],
			right: vec![0.0; MAX_CHUNK_FRAMES],
			record_scratch: vec![0.0; MAX_CHUNK_FRAMES * 2],
			held: [[0; MIDI_CHANNELS]; Source::COUNT],
			sounding: [[[0; KEYS]; MIDI_CHANNELS]; Source::COUNT],
		}
	}

	/// Applies omni redirection (to the sources it is for) and transposition before tracking and the engine.
	fn prepare(&mut self, event: MidiEvent, source: Source) -> MidiEvent {
		let omni = self.shared.omni_channel().filter(|_| source.follows_omni());
		let event = match (omni, event) {
			(Some(target), MidiEvent::NoteOn { key, velocity, .. }) => MidiEvent::NoteOn {
				channel: target,
				key,
				velocity,
			},
			(Some(target), MidiEvent::NoteOff { key, .. }) => MidiEvent::NoteOff { channel: target, key },
			(_, event) => event,
		};
		let transpose = self.shared.transpose();
		let sounding_keys = &mut self.sounding[source as usize];
		match event {
			MidiEvent::NoteOn { channel, key, velocity } => {
				let sounding = (i16::from(key) + i16::from(transpose)).clamp(0, 127) as u8;
				if let Some(map) = sounding_keys.get_mut(usize::from(channel)) {
					map[usize::from(key & 0x7F)] = sounding;
				}
				MidiEvent::NoteOn {
					channel,
					key: sounding,
					velocity,
				}
			}
			MidiEvent::NoteOff { channel, key } => {
				let sounding = sounding_keys
					.get(usize::from(channel))
					.map_or(key, |map| map[usize::from(key & 0x7F)]);
				MidiEvent::NoteOff { channel, key: sounding }
			}
			other => other,
		}
	}

	/// Fills an interleaved output buffer with `channels` channels per frame.
	/// Channels beyond the first two are silent; a mono output gets the L/R average.
	pub fn fill(&mut self, output: &mut [f32], channels: usize) {
		// Clock reads are user-mode on every supported platform (QPC / vDSO), so they are allowed here (N-03).
		let started = Instant::now();
		self.record_mode = self.shared.recording();
		if self.record_mode != RecordMode::Midi {
			self.take_frames = 0;
		}
		// The file first: when playback stops, the UI sends what puts the synthesizer back in order
		// after the player has sent the last of the file — and it must find that already applied.
		self.drain_file_events();
		self.drain_commands();
		self.drain_events();
		let total_frames = output.len() / channels.max(1);
		match self.engine.as_mut() {
			None => output.fill(0.0),
			Some(engine) => {
				let gain = self.shared.master_gain();
				for chunk in output.chunks_mut(MAX_CHUNK_FRAMES * channels) {
					let frames = chunk.len() / channels;
					engine.render(&mut self.left[..frames], &mut self.right[..frames]);
					let (levels, clipped) = measure(&self.left[..frames], &self.right[..frames], gain);
					self.shared.publish_levels(levels, clipped);
					interleave(&self.left[..frames], &self.right[..frames], gain, chunk, channels);
					if self.record_mode == RecordMode::Audio {
						let scratch = &mut self.record_scratch[..frames * 2];
						interleave(&self.left[..frames], &self.right[..frames], gain, scratch, 2);
						let pushed = self.recording.samples.push_slice(scratch);
						self.shared.add_record_dropped((scratch.len() - pushed) as u32);
					}
				}
			}
		}
		// Events are applied at the start of a callback: everything in the next one is this much later.
		if self.record_mode == RecordMode::Midi {
			self.take_frames += total_frames as u64;
		}
		for source in Source::ALL {
			self.shared.publish_notes(source, self.keys_of(source));
		}
		self.shared.publish_stats(
			self.held_count(),
			load_percent(started.elapsed().as_secs_f32(), total_frames, self.sample_rate),
		);
	}

	/// Keeps the held-note maps in sync with what reaches the engine. A note is tracked for its
	/// source; the controllers that silence a channel silence it for every source.
	fn track(&mut self, event: MidiEvent, source: Option<Source>) {
		match (event, source) {
			(MidiEvent::NoteOn { channel, key, .. }, Some(source)) => {
				if let Some(bits) = self.held[source as usize].get_mut(usize::from(channel)) {
					*bits |= 1u128 << (key & 0x7F);
				}
			}
			(MidiEvent::NoteOff { channel, key }, Some(source)) => {
				if let Some(bits) = self.held[source as usize].get_mut(usize::from(channel)) {
					*bits &= !(1u128 << (key & 0x7F));
				}
			}
			(MidiEvent::ControlChange { channel, controller, .. }, _)
				if controller == controller::ALL_NOTES_OFF || controller == controller::ALL_SOUND_OFF =>
			{
				for held in &mut self.held {
					if let Some(bits) = held.get_mut(usize::from(channel)) {
						*bits = 0;
					}
				}
			}
			_ => {}
		}
	}

	fn reset_held(&mut self) {
		self.held = [[0; MIDI_CHANNELS]; Source::COUNT];
	}

	/// The keys `source` holds on any channel, a bit per key.
	fn keys_of(&self, source: Source) -> u128 {
		self.held[source as usize].iter().fold(0, |keys, channel| keys | channel)
	}

	/// Notes held now (F-12 "voices"): a note two sources hold on the same channel counts once.
	fn held_count(&self) -> u32 {
		(0..MIDI_CHANNELS)
			.map(|channel| self.held.iter().fold(0u128, |keys, held| keys | held[channel]).count_ones())
			.sum()
	}

	/// One event on its way to the engine. With a source it is a played note or controller: omni and
	/// transposition apply and the note is tracked for that source. Without one it is a service
	/// message of the UI and goes to the engine as it is. A MIDI take records it as the engine gets it.
	fn feed(&mut self, event: MidiEvent, source: Option<Source>) {
		let event = match source {
			Some(source) => self.prepare(event, source),
			None => event,
		};
		self.track(event, source);
		if self.record_mode == RecordMode::Midi {
			let stamped = StampedEvent {
				frame: self.take_frames,
				event,
			};
			if self.recording.events.try_push(stamped).is_err() {
				self.shared.add_record_dropped(1);
			}
		}
		if let Some(engine) = self.engine.as_mut() {
			apply_event(engine.as_mut(), event);
		}
	}

	fn drain_commands(&mut self) {
		while let Some(command) = self.commands.try_pop() {
			match command {
				Command::Play(event) => self.feed(event, Some(Source::Keyboard)),
				Command::Midi(event) => self.feed(event, None),
				Command::ReplaceEngine(next) => {
					self.reset_held();
					self.shared
						.set_max_voices(next.as_ref().map_or(0, |engine| engine.max_voices() as u32));
					if let Some(previous) = std::mem::replace(&mut self.engine, next) {
						// A full retired ring means the UI thread is not draining; dropping here is the
						// lesser evil compared to leaking, and only happens in that degenerate case.
						let _ = self.retired.try_push(previous);
					}
				}
			}
		}
	}

	fn drain_events(&mut self) {
		while let Some(event) = self.events.try_pop() {
			self.feed(event, Some(Source::MidiIn));
		}
	}

	fn drain_file_events(&mut self) {
		while let Some(event) = self.file_events.try_pop() {
			self.feed(event, Some(Source::File));
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

/// Callback duration relative to the audio time of the block, in percent (F-12 "cpu").
fn load_percent(elapsed_seconds: f32, frames: usize, sample_rate: u32) -> u32 {
	if frames == 0 || sample_rate == 0 {
		return 0;
	}
	let block_seconds = frames as f32 / sample_rate as f32;
	(elapsed_seconds / block_seconds * 100.0).round().clamp(0.0, 999.0) as u32
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
mod tests;
