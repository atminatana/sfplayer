//! The built-in synthesizer (spec v1.2, F-22): one timbre for all sixteen channels — a sine, a saw or a
//! triangle shaped by an ADSR envelope — with 64 voices, a reverb and a chorus.
//!
//! Everything is allocated when the engine is created; no method called on the audio thread allocates
//! (spec N-03). The timbre (`timbre.rs`) is shared with the UI through atomics: a note takes the wave
//! and the envelope that are set at the moment it starts, so changing them needs no new engine and
//! does not disturb the notes already sounding.

use std::sync::Arc;

use super::SynthEngine;
use super::effects::Effects;
use super::timbre::{Timbre, Wave};
use crate::midi::controller;

/// Polyphony: a note beyond it takes the voice of the oldest one.
pub const MAX_VOICES: usize = 64;
const CHANNELS: usize = 16;
/// How far Pitch Bend reaches each way, in semitones.
const BEND_RANGE_SEMITONES: f32 = 2.0;
const BEND_CENTRE: f32 = 8192.0;
/// Amplitude of one voice at full velocity and channel volume: headroom for chords.
const VOICE_GAIN: f32 = 0.2;
/// Channel volume (CC 7) before any is received, as General MIDI has it.
const DEFAULT_VOLUME: u8 = 100;
/// CC 64 values from here up mean the pedal is down.
const PEDAL_DOWN: u8 = 64;
const CONCERT_A_KEY: f32 = 69.0;
const CONCERT_A_HZ: f32 = 440.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
	Idle,
	Attack,
	Decay,
	Sustain,
	Release,
}

#[derive(Debug, Clone, Copy)]
struct Voice {
	stage: Stage,
	channel: u8,
	key: u8,
	wave: Wave,
	/// Velocity as amplitude, the voice gain included.
	gain: f32,
	/// Position in the period, 0..1.
	phase: f32,
	/// Periods per sample at the pitch of the key, Pitch Bend not applied.
	step: f32,
	/// The envelope now, 0..1.
	level: f32,
	/// Envelope travelled per sample while it rises, while it falls to the sustain level, and after
	/// the release; the last one is set when the release begins.
	attack_step: f32,
	decay_step: f32,
	release_step: f32,
	sustain: f32,
	/// Release time of the envelope the note started with, in samples.
	release_samples: f32,
	/// The key is up, the pedal keeps the note.
	is_held_by_pedal: bool,
	/// Order of starting: the lowest belongs to the oldest note.
	stamp: u64,
}

impl Voice {
	const IDLE: Self = Self {
		stage: Stage::Idle,
		channel: 0,
		key: 0,
		wave: Wave::Sine,
		gain: 0.0,
		phase: 0.0,
		step: 0.0,
		level: 0.0,
		attack_step: 0.0,
		decay_step: 0.0,
		release_step: 0.0,
		sustain: 0.0,
		release_samples: 1.0,
		is_held_by_pedal: false,
		stamp: 0,
	};

	fn is_active(&self) -> bool {
		self.stage != Stage::Idle
	}

	/// Whether the key of this voice is still down, or kept down by the pedal.
	fn is_sounding_key(&self, channel: u8, key: u8) -> bool {
		self.is_active() && self.stage != Stage::Release && self.channel == channel && self.key == key
	}

	/// The envelope falls from where it is to silence in the release time.
	fn release(&mut self) {
		if self.is_active() && self.stage != Stage::Release {
			self.stage = Stage::Release;
			self.release_step = self.level / self.release_samples;
			self.is_held_by_pedal = false;
		}
	}

	/// Moves the envelope one sample on and returns its level for this sample.
	fn advance_envelope(&mut self) -> f32 {
		let level = self.level;
		match self.stage {
			Stage::Attack => {
				self.level += self.attack_step;
				if self.level >= 1.0 {
					self.level = 1.0;
					self.stage = Stage::Decay;
				}
			}
			Stage::Decay => {
				self.level -= self.decay_step;
				if self.level <= self.sustain {
					self.level = self.sustain;
					self.stage = Stage::Sustain;
				}
			}
			Stage::Release => {
				self.level -= self.release_step;
				if self.level <= 0.0 {
					self.level = 0.0;
					self.stage = Stage::Idle;
				}
			}
			Stage::Sustain | Stage::Idle => {}
		}
		level
	}
}

#[derive(Debug, Clone, Copy)]
struct Channel {
	/// CC 7 as amplitude.
	volume: f32,
	/// Pitch Bend as a frequency ratio.
	bend: f32,
	is_pedal_down: bool,
	/// How much of the channel goes to the reverb (CC 91) and to the chorus (CC 93), as gain.
	reverb_send: f32,
	chorus_send: f32,
}

pub struct BuiltinSynth {
	sample_rate: f32,
	timbre: Arc<Timbre>,
	voices: [Voice; MAX_VOICES],
	channels: [Channel; CHANNELS],
	/// Notes started so far: the stamp of the next one.
	started: u64,
	effects: Effects,
}

impl BuiltinSynth {
	/// `timbre` stays shared: what the UI sets in it applies to the notes that start afterwards.
	/// Both effects are silent until a channel is given a send level.
	pub fn new(sample_rate: u32, timbre: Arc<Timbre>) -> Self {
		Self {
			sample_rate: sample_rate.max(1) as f32,
			timbre,
			voices: [Voice::IDLE; MAX_VOICES],
			channels: [Channel {
				volume: amplitude_of(DEFAULT_VOLUME),
				bend: 1.0,
				is_pedal_down: false,
				reverb_send: 0.0,
				chorus_send: 0.0,
			}; CHANNELS],
			started: 0,
			effects: Effects::new(sample_rate.max(1)),
		}
	}

	/// The voice for a new note: the one already playing this key, else a free one, else the oldest.
	fn voice_for(&mut self, channel: u8, key: u8) -> &mut Voice {
		let same_key = self.voices.iter().position(|voice| voice.is_sounding_key(channel, key));
		let free = || self.voices.iter().position(|voice| !voice.is_active());
		let oldest = || {
			self.voices
				.iter()
				.enumerate()
				.min_by_key(|(_, voice)| voice.stamp)
				.map_or(0, |(index, _)| index)
		};
		let index = same_key.or_else(free).unwrap_or_else(oldest);
		&mut self.voices[index]
	}

	fn voices_of(&mut self, channel: u8) -> impl Iterator<Item = &mut Voice> {
		self.voices
			.iter_mut()
			.filter(move |voice| voice.is_active() && voice.channel == channel)
	}

	/// Renders one block of at most `Effects::BLOCK` frames: the voices, then what the effects make
	/// of the share of each voice its channel sends them.
	fn render_block(&mut self, left: &mut [f32], right: &mut [f32]) {
		left.fill(0.0);
		let (reverb_input, chorus_input) = self.effects.begin_block(left.len());
		for voice in self.voices.iter_mut().filter(|voice| voice.is_active()) {
			let channel = self.channels[usize::from(voice.channel)];
			let step = voice.step * channel.bend;
			let gain = voice.gain * channel.volume;
			for (frame, sample) in left.iter_mut().enumerate() {
				let level = voice.advance_envelope();
				let value = voice.wave.sample(voice.phase, step) * level * gain;
				*sample += value;
				reverb_input[frame] += value * channel.reverb_send;
				chorus_input[frame] += value * channel.chorus_send;
				voice.phase += step;
				if voice.phase >= 1.0 {
					voice.phase -= 1.0;
				}
				if !voice.is_active() {
					break;
				}
			}
		}
		// The timbre has no width: both sides carry the same signal, and only the effects tell them apart.
		right.copy_from_slice(left);
		self.effects.finish_block(left, right);
	}
}

/// A 7-bit value (velocity, volume) as amplitude: the square, so that half the value is a quarter
/// of the amplitude — close to how loudness is heard.
fn amplitude_of(value: u8) -> f32 {
	let share = f32::from(value.min(127)) / 127.0;
	share * share
}

/// A send level (CC 91, CC 93) as the share of the signal that is sent: 127 sends all of it.
fn send_of(value: u8) -> f32 {
	f32::from(value.min(127)) / 127.0
}

fn frequency_of(key: u8) -> f32 {
	CONCERT_A_HZ * 2f32.powf((f32::from(key) - CONCERT_A_KEY) / 12.0)
}

impl SynthEngine for BuiltinSynth {
	fn note_on(&mut self, channel: u8, key: u8, velocity: u8) {
		if usize::from(channel) >= CHANNELS || key > 127 {
			return;
		}
		if velocity == 0 {
			self.note_off(channel, key);
			return;
		}
		let wave = self.timbre.wave();
		let adsr = self.timbre.adsr().clamped();
		let sample_rate = self.sample_rate;
		let stamp = self.started;
		self.started += 1;
		let voice = self.voice_for(channel, key);
		// A voice that goes on with the same key keeps its place in the period and rises from where its
		// envelope is: no click. Any other voice starts from silence.
		if !voice.is_sounding_key(channel, key) {
			voice.phase = 0.0;
			voice.level = 0.0;
		}
		voice.stage = Stage::Attack;
		voice.channel = channel;
		voice.key = key;
		voice.wave = wave;
		voice.gain = amplitude_of(velocity) * VOICE_GAIN;
		voice.step = frequency_of(key) / sample_rate;
		voice.attack_step = 1.0 / (adsr.attack * sample_rate);
		voice.decay_step = (1.0 - adsr.sustain) / (adsr.decay * sample_rate);
		voice.sustain = adsr.sustain;
		voice.release_samples = adsr.release * sample_rate;
		voice.is_held_by_pedal = false;
		voice.stamp = stamp;
	}

	fn note_off(&mut self, channel: u8, key: u8) {
		let Some(is_pedal_down) = self.channels.get(usize::from(channel)).map(|channel| channel.is_pedal_down) else {
			return;
		};
		for voice in self.voices.iter_mut().filter(|voice| voice.is_sounding_key(channel, key)) {
			if is_pedal_down {
				voice.is_held_by_pedal = true;
			} else {
				voice.release();
			}
		}
	}

	fn control_change(&mut self, channel: u8, controller: u8, value: u8) {
		let Some(state) = self.channels.get_mut(usize::from(channel)) else {
			return;
		};
		match controller {
			controller::VOLUME => state.volume = amplitude_of(value),
			controller::REVERB_SEND => state.reverb_send = send_of(value) * Effects::REVERB_SEND_GAIN,
			controller::CHORUS_SEND => state.chorus_send = send_of(value),
			controller::SUSTAIN => {
				state.is_pedal_down = value >= PEDAL_DOWN;
				if !state.is_pedal_down {
					for voice in self.voices_of(channel).filter(|voice| voice.is_held_by_pedal) {
						voice.release();
					}
				}
			}
			controller::ALL_SOUND_OFF => {
				for voice in self.voices_of(channel) {
					*voice = Voice::IDLE;
				}
			}
			controller::ALL_NOTES_OFF => {
				for voice in self.voices_of(channel) {
					voice.release();
				}
			}
			_ => {}
		}
	}

	/// One timbre for everything (spec v1.2, F-22): there are no programs to change.
	fn program_change(&mut self, _channel: u8, _program: u8) {}

	fn pitch_bend(&mut self, channel: u8, value: u16) {
		if let Some(state) = self.channels.get_mut(usize::from(channel)) {
			let share = (f32::from(value.min(16_383)) - BEND_CENTRE) / BEND_CENTRE;
			state.bend = 2f32.powf(share * BEND_RANGE_SEMITONES / 12.0);
		}
	}

	fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
		let frames = left.len().min(right.len());
		// The contract is two buffers of one length; whatever one of them has beyond it is silence.
		left[frames..].fill(0.0);
		right[frames..].fill(0.0);
		let blocks = left[..frames]
			.chunks_mut(Effects::BLOCK)
			.zip(right[..frames].chunks_mut(Effects::BLOCK));
		for (left, right) in blocks {
			self.render_block(left, right);
		}
	}

	fn max_voices(&self) -> usize {
		MAX_VOICES
	}
}

#[cfg(test)]
mod tests;
