//! The timbre of the built-in synthesizer (spec v1.2, F-22): the wave, the ADSR envelope with its
//! ready-made presets, and `Timbre` — the two as atomics, set by the UI and read by the engine when a
//! note starts.

use std::f32::consts::TAU;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
	#[default]
	Sine,
	Saw,
	Triangle,
}

impl Wave {
	const ALL: [Self; 3] = [Self::Sine, Self::Saw, Self::Triangle];

	/// One sample at `phase` (0..1 of the period); `step` is the phase travelled per sample.
	pub(super) fn sample(self, phase: f32, step: f32) -> f32 {
		match self {
			Self::Sine => (phase * TAU).sin(),
			// The jump of a plain saw rings at every multiple of the pitch up to infinity; folded back
			// by sampling, that is audible. The correction smooths the two samples around the jump.
			Self::Saw => 2.0 * phase - 1.0 - jump_correction(phase, step),
			Self::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
		}
	}
}

/// The polynomial band-limited step ("polyBLEP") for a jump at phase 0.
fn jump_correction(phase: f32, step: f32) -> f32 {
	if step <= 0.0 {
		0.0
	} else if phase < step {
		let x = phase / step;
		2.0 * x - x * x - 1.0
	} else if phase > 1.0 - step {
		let x = (phase - 1.0) / step;
		x * x + 2.0 * x + 1.0
	} else {
		0.0
	}
}

/// The envelope of a note: times in seconds, the sustain level as a share of the peak.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adsr {
	pub attack: f32,
	pub decay: f32,
	pub sustain: f32,
	pub release: f32,
}

impl Adsr {
	/// Limits of the three times (spec v1.2, F-22).
	pub const MIN_TIME: f32 = 0.001;
	pub const MAX_TIME: f32 = 2.0;

	/// The same envelope with every value inside its limits; a value that is not a number becomes the
	/// lowest allowed.
	pub fn clamped(self) -> Self {
		let time = |seconds: f32| {
			if seconds.is_nan() {
				Self::MIN_TIME
			} else {
				seconds.clamp(Self::MIN_TIME, Self::MAX_TIME)
			}
		};
		Self {
			attack: time(self.attack),
			decay: time(self.decay),
			sustain: if self.sustain.is_nan() { 0.0 } else { self.sustain.clamp(0.0, 1.0) },
			release: time(self.release),
		}
	}
}

/// The envelopes offered ready-made (spec v1.2, F-22); `Custom` is whatever the user has set by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdsrPreset {
	#[default]
	Piano,
	Harpsichord,
	Organ,
	Flute,
	Custom,
}

impl AdsrPreset {
	/// The four ready-made ones, in the order the screen shows them.
	pub const READY_MADE: [Self; 4] = [Self::Piano, Self::Harpsichord, Self::Organ, Self::Flute];

	/// The envelope of a ready-made preset; `Custom` has none of its own.
	pub fn adsr(self) -> Option<Adsr> {
		let (attack, decay, sustain, release) = match self {
			Self::Piano => (0.003, 0.5, 0.35, 0.35),
			Self::Harpsichord => (0.002, 0.28, 0.2, 0.18),
			Self::Organ => (0.03, 0.06, 1.0, 0.09),
			Self::Flute => (0.12, 0.09, 0.85, 0.22),
			Self::Custom => return None,
		};
		Some(Adsr {
			attack,
			decay,
			sustain,
			release,
		})
	}
}

/// The timbre the UI sets and the engine reads when a note starts. Lock-free: every value is an atomic.
#[derive(Debug)]
pub struct Timbre {
	wave: AtomicU8,
	/// `f32` bits of attack, decay, sustain, release.
	adsr: [AtomicU32; 4],
}

impl Timbre {
	pub fn new(wave: Wave, adsr: Adsr) -> Self {
		let timbre = Self {
			wave: AtomicU8::new(0),
			adsr: [const { AtomicU32::new(0) }; 4],
		};
		timbre.set_wave(wave);
		timbre.set_adsr(adsr);
		timbre
	}

	pub fn set_wave(&self, wave: Wave) {
		self.wave.store(wave as u8, Ordering::Relaxed);
	}

	pub fn wave(&self) -> Wave {
		Wave::ALL
			.get(usize::from(self.wave.load(Ordering::Relaxed)))
			.copied()
			.unwrap_or(Wave::Sine)
	}

	/// Stores the envelope with its values brought inside their limits.
	pub fn set_adsr(&self, adsr: Adsr) {
		let adsr = adsr.clamped();
		for (slot, value) in self.adsr.iter().zip([adsr.attack, adsr.decay, adsr.sustain, adsr.release]) {
			slot.store(value.to_bits(), Ordering::Relaxed);
		}
	}

	pub fn adsr(&self) -> Adsr {
		let read = |index: usize| f32::from_bits(self.adsr[index].load(Ordering::Relaxed));
		Adsr {
			attack: read(0),
			decay: read(1),
			sustain: read(2),
			release: read(3),
		}
	}
}
