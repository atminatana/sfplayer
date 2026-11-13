//! Offline tests of the built-in synthesizer: what it renders is measured, nothing is listened to.

use super::*;
use crate::synth::test_support::rms;

const SAMPLE_RATE: u32 = 48_000;
const SECOND: usize = SAMPLE_RATE as usize;
const FULL: u8 = 127;
const A4: u8 = 69;
/// An envelope that is at its full level within a millisecond and gone within one: for measuring the wave.
const ORGAN: Adsr = Adsr {
	attack: 0.001,
	decay: 0.001,
	sustain: 1.0,
	release: 0.001,
};

/// An engine with every channel at full volume, so that a full-velocity voice peaks at `VOICE_GAIN`.
fn synth(wave: Wave, adsr: Adsr) -> (BuiltinSynth, Arc<Timbre>) {
	let timbre = Arc::new(Timbre::new(wave, adsr));
	let mut synth = BuiltinSynth::new(SAMPLE_RATE, Arc::clone(&timbre));
	for channel in 0..16 {
		synth.control_change(channel, controller::VOLUME, FULL);
	}
	(synth, timbre)
}

/// Renders `frames` frames and returns one side; the two sides are the same signal.
fn render(synth: &mut BuiltinSynth, frames: usize) -> Vec<f32> {
	let mut left = vec![9.0; frames];
	let mut right = vec![-9.0; frames];
	synth.render(&mut left, &mut right);
	assert_eq!(left, right);
	left
}

fn peak(samples: &[f32]) -> f32 {
	samples.iter().fold(0.0, |peak, sample| peak.max(sample.abs()))
}

/// Times the signal goes from below zero to zero or above: once per period for all three waves.
fn rising_crossings(samples: &[f32]) -> usize {
	samples.windows(2).filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0).count()
}

fn is_silent(samples: &[f32]) -> bool {
	samples.iter().all(|sample| *sample == 0.0)
}

fn active(synth: &BuiltinSynth) -> usize {
	synth.voices.iter().filter(|voice| voice.is_active()).count()
}

fn has_key(synth: &BuiltinSynth, key: u8) -> bool {
	synth.voices.iter().any(|voice| voice.is_active() && voice.key == key)
}

/// The envelope around `seconds` into `samples`: the peak of five milliseconds there, as a share of the
/// amplitude of a full voice.
fn envelope_at(samples: &[f32], seconds: f32) -> f32 {
	let centre = (seconds * SAMPLE_RATE as f32) as usize;
	let half = SECOND / 400;
	peak(&samples[centre - half..centre + half]) / VOICE_GAIN
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, what: &str) {
	assert!(
		(actual - expected).abs() <= tolerance,
		"{what}: {actual}, expected {expected} ± {tolerance}"
	);
}

#[test]
fn every_wave_sounds_at_the_pitch_of_its_key() {
	for wave in [Wave::Sine, Wave::Saw, Wave::Triangle] {
		for (key, hertz) in [(A4, 440), (A4 + 12, 880), (60, 262), (A4 - 24, 110)] {
			let (mut synth, _) = synth(wave, ORGAN);
			synth.note_on(0, key, FULL);
			render(&mut synth, SECOND / 10);
			let second = render(&mut synth, SECOND);
			assert!(rms(&second) > 0.05, "{wave:?} key {key} is audible: {}", rms(&second));
			let periods = rising_crossings(&second);
			assert!(
				periods.abs_diff(hertz) <= 1,
				"{wave:?} key {key}: {periods} periods in a second, expected {hertz}"
			);
		}
	}
}

#[test]
fn the_three_waves_have_their_own_shapes() {
	let shape = |wave: Wave| {
		let (mut synth, _) = synth(wave, ORGAN);
		synth.note_on(0, A4, FULL);
		render(&mut synth, SECOND / 10);
		let second = render(&mut synth, SECOND);
		let top = peak(&second);
		let largest_step = second.windows(2).fold(0.0f32, |step, pair| step.max((pair[1] - pair[0]).abs()));
		(top, rms(&second) / top, largest_step / top)
	};
	let (sine_peak, sine_fill, sine_step) = shape(Wave::Sine);
	let (saw_peak, saw_fill, saw_step) = shape(Wave::Saw);
	let (triangle_peak, triangle_fill, triangle_step) = shape(Wave::Triangle);
	for (name, top) in [("sine", sine_peak), ("saw", saw_peak), ("triangle", triangle_peak)] {
		assert_close(top, VOICE_GAIN, 0.01, name);
	}
	// A sine fills 1/√2 of its peak, a saw and a triangle 1/√3.
	assert_close(sine_fill, 0.707, 0.01, "sine");
	assert_close(saw_fill, 0.577, 0.02, "saw");
	assert_close(triangle_fill, 0.577, 0.01, "triangle");
	// Only the saw jumps. Smoothed, the jump is spread over two samples: at worst three quarters of
	// the swing in one step (1.5 of the peak), where a plain saw would cover nearly all of it (1.98).
	assert!(saw_step > 0.8 && saw_step < 1.7, "saw {saw_step}");
	assert!(sine_step < 0.1 && triangle_step < 0.1, "sine {sine_step}, triangle {triangle_step}");
}

#[test]
fn the_saw_is_a_ramp_whose_jump_is_smoothed() {
	let step = 0.01;
	// Away from the jump it is the plain ramp from -1 to 1.
	assert_close(Wave::Saw.sample(0.25, step), -0.5, 1e-6, "a quarter of the way");
	assert_close(Wave::Saw.sample(0.5, step), 0.0, 1e-6, "the middle");
	assert_close(Wave::Saw.sample(0.75, step), 0.5, 1e-6, "three quarters");
	// At the jump itself the sample is half way between the top and the bottom, and the samples on
	// either side are pulled towards it.
	assert_close(Wave::Saw.sample(0.0, step), 0.0, 1e-6, "on the jump");
	let before = Wave::Saw.sample(1.0 - step / 2.0, step);
	let after = Wave::Saw.sample(step / 2.0, step);
	assert!(before > 0.5 && before < 0.99, "just before the jump: {before}");
	assert!(after < -0.5 && after > -0.99, "just after the jump: {after}");
	// A wave that does not move is not corrected.
	assert_close(Wave::Saw.sample(0.0, 0.0), -1.0, 1e-6, "no step");
}

#[test]
fn the_envelope_goes_through_its_four_stages_in_the_set_times() {
	let adsr = Adsr {
		attack: 0.1,
		decay: 0.2,
		sustain: 0.5,
		release: 0.3,
	};
	let (mut synth, _) = synth(Wave::Sine, adsr);
	synth.note_on(0, A4 + 12, FULL);
	let held = render(&mut synth, SECOND / 2);
	assert!(held[0].abs() < 1e-6, "a note starts from silence");
	assert_close(envelope_at(&held, 0.05), 0.5, 0.05, "half way up the attack");
	assert_close(envelope_at(&held, 0.1), 1.0, 0.03, "the peak at the end of the attack");
	assert_close(envelope_at(&held, 0.2), 0.75, 0.05, "half way down the decay");
	assert_close(envelope_at(&held, 0.3), 0.5, 0.03, "the sustain level at the end of the decay");
	assert_close(envelope_at(&held, 0.45), 0.5, 0.01, "the sustain level holds");

	synth.note_off(0, A4 + 12);
	let released = render(&mut synth, SECOND / 2);
	assert_close(envelope_at(&released, 0.15), 0.25, 0.05, "half way down the release");
	assert!(envelope_at(&released, 0.295) < 0.03, "nearly gone at the end of the release");
	assert!(is_silent(&released[SECOND * 31 / 100..]), "silence once the release is over");
	assert_eq!(active(&synth), 0, "the voice is free again");
}

#[test]
fn a_release_takes_the_release_time_from_wherever_the_envelope_is() {
	let adsr = Adsr {
		attack: 0.2,
		decay: 0.2,
		sustain: 0.5,
		release: 0.1,
	};
	let (mut synth, _) = synth(Wave::Sine, adsr);
	synth.note_on(0, A4 + 12, FULL);
	// Let go a quarter of the way up the attack.
	render(&mut synth, SECOND / 20);
	synth.note_off(0, A4 + 12);
	let released = render(&mut synth, SECOND / 5);
	assert!(peak(&released[..SECOND / 100]) / VOICE_GAIN < 0.3, "it does not go on rising");
	assert!(envelope_at(&released, 0.05) > 0.05, "still sounding half way through the release");
	assert!(is_silent(&released[SECOND * 11 / 100..]));
}

#[test]
fn the_sixty_fifth_note_takes_the_voice_of_the_oldest() {
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	assert_eq!(synth.max_voices(), 64);
	for key in 20..84 {
		synth.note_on(0, key, FULL);
	}
	assert_eq!(active(&synth), MAX_VOICES);
	synth.note_on(0, 100, FULL);
	assert_eq!(active(&synth), MAX_VOICES, "no more voices than the limit");
	assert!(!has_key(&synth, 20), "the first note gave its voice away");
	assert!(has_key(&synth, 21) && has_key(&synth, 83) && has_key(&synth, 100));
	synth.note_on(0, 101, FULL);
	assert!(!has_key(&synth, 21) && has_key(&synth, 22), "then the second one");

	// A voice that finished is taken before any sounding one is.
	synth.note_off(0, 50);
	render(&mut synth, SECOND / 100);
	assert_eq!(active(&synth), MAX_VOICES - 1);
	synth.note_on(0, 102, FULL);
	assert!(
		has_key(&synth, 22) && has_key(&synth, 102),
		"nothing was taken from a sounding note"
	);
}

#[test]
fn a_key_struck_again_goes_on_in_its_own_voice_without_a_break() {
	let adsr = Adsr {
		attack: 0.1,
		decay: 0.1,
		sustain: 0.5,
		release: 0.1,
	};
	let (mut synth, _) = synth(Wave::Sine, adsr);
	synth.note_on(0, A4, FULL);
	let before = render(&mut synth, SECOND / 2);
	synth.note_on(0, A4, FULL);
	assert_eq!(active(&synth), 1, "the same key does not take a second voice");
	let after = render(&mut synth, SECOND / 2);
	let seam = (after[0] - before[before.len() - 1]).abs();
	assert!(seam < 0.01, "the signal goes on from where it was: step {seam}");
	assert_close(
		envelope_at(&after, 0.05),
		1.0,
		0.05,
		"and rises again from the sustain level to the peak",
	);

	// The same key on another channel is another note.
	synth.note_on(1, A4, FULL);
	assert_eq!(active(&synth), 2);
}

#[test]
fn the_pedal_keeps_released_notes_until_it_comes_up() {
	let adsr = Adsr {
		attack: 0.001,
		decay: 0.001,
		sustain: 1.0,
		release: 0.05,
	};
	let (mut synth, _) = synth(Wave::Sine, adsr);
	synth.control_change(0, controller::SUSTAIN, 127);
	synth.note_on(0, A4, FULL);
	synth.note_on(1, A4 + 12, FULL);
	render(&mut synth, SECOND / 10);
	synth.note_off(0, A4);
	synth.note_off(1, A4 + 12);
	let held = render(&mut synth, SECOND / 2);
	assert_close(envelope_at(&held, 0.4), 1.0, 0.02, "the pedalled channel still sounds in full");
	assert_eq!(active(&synth), 1, "the channel without the pedal let its note go");

	synth.control_change(0, controller::SUSTAIN, 63);
	let released = render(&mut synth, SECOND / 5);
	assert!(is_silent(&released[SECOND * 6 / 100..]), "pedal up: the note is released");

	// A key still down when the pedal comes up keeps sounding.
	synth.control_change(0, controller::SUSTAIN, 64);
	synth.note_on(0, A4, FULL);
	synth.control_change(0, controller::SUSTAIN, 0);
	let still = render(&mut synth, SECOND / 5);
	assert_close(envelope_at(&still, 0.15), 1.0, 0.02, "the key holds its own note");

	// The pedal is down from value 64: a key released then is kept.
	synth.control_change(0, controller::SUSTAIN, 64);
	synth.note_off(0, A4);
	let kept = render(&mut synth, SECOND / 5);
	assert_close(envelope_at(&kept, 0.15), 1.0, 0.02, "64 is the pedal down");
}

#[test]
fn pitch_bend_reaches_two_semitones_each_way_on_its_channel_only() {
	let periods_with = |bend: Option<u16>, bent_channel: u8| {
		let (mut synth, _) = synth(Wave::Sine, ORGAN);
		if let Some(value) = bend {
			synth.pitch_bend(bent_channel, value);
		}
		synth.note_on(0, A4, FULL);
		render(&mut synth, SECOND / 10);
		rising_crossings(&render(&mut synth, SECOND))
	};
	assert!(periods_with(None, 0).abs_diff(440) <= 1);
	assert!(periods_with(Some(8192), 0).abs_diff(440) <= 1, "the centre is no bend");
	assert!(periods_with(Some(16_383), 0).abs_diff(494) <= 1, "B4, two semitones up");
	assert!(periods_with(Some(0), 0).abs_diff(392) <= 1, "G4, two semitones down");
	assert!(periods_with(Some(12_288), 0).abs_diff(466) <= 1, "half way up is one semitone: A#4");
	assert!(periods_with(Some(16_383), 1).abs_diff(440) <= 1, "a bend on another channel");

	// The bend moves a note that is already sounding.
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.note_on(0, A4, FULL);
	render(&mut synth, SECOND / 10);
	synth.pitch_bend(0, 16_383);
	assert!(rising_crossings(&render(&mut synth, SECOND)).abs_diff(494) <= 1);
}

#[test]
fn velocity_and_channel_volume_set_the_loudness() {
	let steady_peak = |velocity: u8, volume: Option<u8>| {
		let timbre = Arc::new(Timbre::new(Wave::Sine, ORGAN));
		let mut synth = BuiltinSynth::new(SAMPLE_RATE, timbre);
		if let Some(volume) = volume {
			synth.control_change(0, controller::VOLUME, volume);
		}
		synth.note_on(0, A4, velocity);
		render(&mut synth, SECOND / 10);
		peak(&render(&mut synth, SECOND / 10))
	};
	let full = steady_peak(FULL, Some(FULL));
	assert_close(full, VOICE_GAIN, 0.002, "full velocity at full volume");
	// Loudness follows the square of the value: half of 127 is about a quarter of the amplitude.
	assert_close(steady_peak(64, Some(FULL)) / full, (64.0f32 / 127.0).powi(2), 0.005, "velocity 64");
	assert_close(steady_peak(FULL, Some(64)) / full, (64.0f32 / 127.0).powi(2), 0.005, "volume 64");
	assert_close(
		steady_peak(FULL, None) / full,
		(100.0f32 / 127.0).powi(2),
		0.005,
		"volume 100 until the channel is told otherwise",
	);
	assert_eq!(steady_peak(FULL, Some(0)), 0.0, "volume 0 is silence");
	assert!(steady_peak(1, Some(FULL)) > 0.0, "the softest note still sounds");
}

#[test]
fn volume_follows_the_controller_while_a_note_sounds() {
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.note_on(0, A4, FULL);
	render(&mut synth, SECOND / 10);
	synth.control_change(0, controller::VOLUME, 0);
	assert!(is_silent(&render(&mut synth, SECOND / 10)));
	synth.control_change(0, controller::VOLUME, FULL);
	assert_close(peak(&render(&mut synth, SECOND / 10)), VOICE_GAIN, 0.002, "back at full volume");
}

#[test]
fn all_sound_off_cuts_and_all_notes_off_releases_one_channel() {
	let adsr = Adsr {
		attack: 0.001,
		decay: 0.001,
		sustain: 1.0,
		release: 0.1,
	};
	let (mut synth, _) = synth(Wave::Sine, adsr);
	synth.note_on(0, A4, FULL);
	synth.note_on(0, A4 + 4, FULL);
	synth.note_on(5, A4 + 7, FULL);
	render(&mut synth, SECOND / 10);

	synth.control_change(0, controller::ALL_SOUND_OFF, 0);
	assert_eq!(active(&synth), 1, "channel 1 is cut at once, channel 6 plays on");
	assert!(has_key(&synth, A4 + 7));

	synth.control_change(5, controller::ALL_NOTES_OFF, 0);
	assert_eq!(active(&synth), 1, "released, not cut: the note fades");
	let fading = render(&mut synth, SECOND / 5);
	assert!(envelope_at(&fading, 0.05) > 0.3);
	assert!(is_silent(&fading[SECOND * 11 / 100..]));
	assert_eq!(active(&synth), 0);

	// All Notes Off lets go of what the pedal held, too.
	synth.control_change(0, controller::SUSTAIN, 127);
	synth.note_on(0, A4, FULL);
	synth.note_off(0, A4);
	synth.control_change(0, controller::ALL_NOTES_OFF, 0);
	render(&mut synth, SECOND / 5);
	assert_eq!(active(&synth), 0);
}

#[test]
fn a_change_of_timbre_applies_to_the_notes_that_start_after_it() {
	let (mut synth, timbre) = synth(Wave::Sine, ORGAN);
	synth.note_on(0, A4, FULL);
	timbre.set_wave(Wave::Saw);
	timbre.set_adsr(Adsr {
		attack: 0.5,
		decay: 0.1,
		sustain: 0.2,
		release: 1.0,
	});
	synth.note_on(0, A4 + 12, FULL);
	let waves: Vec<Wave> = synth
		.voices
		.iter()
		.filter(|voice| voice.is_active())
		.map(|voice| voice.wave)
		.collect();
	assert_eq!(waves, [Wave::Sine, Wave::Saw], "the sounding note keeps the wave it started with");
	let sustains: Vec<f32> = synth
		.voices
		.iter()
		.filter(|voice| voice.is_active())
		.map(|voice| voice.sustain)
		.collect();
	assert_eq!(sustains, [1.0, 0.2], "and its envelope");
}

#[test]
fn every_channel_plays_the_same_timbre_and_what_is_out_of_range_is_ignored() {
	let on_channel = |channel: u8| {
		let (mut synth, _) = synth(Wave::Triangle, ORGAN);
		synth.program_change(channel, 42);
		synth.note_on(channel, A4, 90);
		render(&mut synth, SECOND / 10)
	};
	let first = on_channel(0);
	assert!(rms(&first) > 0.01);
	// Channel 10 is not a drum kit here, and a program change selects nothing.
	assert_eq!(on_channel(9), first);
	assert_eq!(on_channel(15), first);

	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.note_on(16, A4, FULL);
	synth.note_on(0, 128, FULL);
	synth.note_off(16, A4);
	synth.control_change(16, controller::VOLUME, 0);
	synth.pitch_bend(16, 0);
	assert_eq!(active(&synth), 0);
	assert!(is_silent(&render(&mut synth, 256)));
}

#[test]
fn a_note_on_with_velocity_zero_is_a_note_off() {
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.note_on(0, A4, FULL);
	render(&mut synth, SECOND / 10);
	synth.note_on(0, A4, 0);
	render(&mut synth, SECOND / 100);
	assert_eq!(active(&synth), 0);
}

#[test]
fn notes_add_up() {
	let alone = |key: u8| {
		let (mut synth, _) = synth(Wave::Sine, ORGAN);
		synth.note_on(0, key, FULL);
		render(&mut synth, SECOND / 10)
	};
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.note_on(0, A4, FULL);
	synth.note_on(0, A4 + 7, FULL);
	let together = render(&mut synth, SECOND / 10);
	let (first, second) = (alone(A4), alone(A4 + 7));
	for (index, sample) in together.iter().enumerate() {
		assert!((sample - (first[index] + second[index])).abs() < 1e-5, "sample {index}");
	}
}

#[test]
fn a_block_of_any_length_continues_the_signal() {
	let whole = {
		let (mut synth, _) = synth(Wave::Saw, ORGAN);
		synth.note_on(0, A4, FULL);
		render(&mut synth, 1000)
	};
	let (mut synth, _) = synth(Wave::Saw, ORGAN);
	synth.note_on(0, A4, FULL);
	let mut pieces = Vec::new();
	for frames in [1, 0, 255, 256, 488] {
		pieces.extend(render(&mut synth, frames));
	}
	assert_eq!(pieces, whole);
}

#[test]
fn an_envelope_is_brought_inside_its_limits() {
	let wild = Adsr {
		attack: 0.0,
		decay: 99.0,
		sustain: 1.5,
		release: -1.0,
	};
	assert_eq!(
		wild.clamped(),
		Adsr {
			attack: Adsr::MIN_TIME,
			decay: Adsr::MAX_TIME,
			sustain: 1.0,
			release: Adsr::MIN_TIME,
		}
	);
	let not_numbers = Adsr {
		attack: f32::NAN,
		decay: f32::INFINITY,
		sustain: f32::NAN,
		release: f32::NEG_INFINITY,
	};
	assert_eq!(
		not_numbers.clamped(),
		Adsr {
			attack: Adsr::MIN_TIME,
			decay: Adsr::MAX_TIME,
			sustain: 0.0,
			release: Adsr::MIN_TIME,
		}
	);
	let fine = Adsr {
		attack: 0.003,
		decay: 0.5,
		sustain: 0.35,
		release: 0.35,
	};
	assert_eq!(fine.clamped(), fine);
}

#[test]
fn the_timbre_keeps_what_is_set_in_it() {
	let piano = Adsr {
		attack: 0.003,
		decay: 0.5,
		sustain: 0.35,
		release: 0.35,
	};
	let timbre = Timbre::new(Wave::Triangle, piano);
	assert_eq!((timbre.wave(), timbre.adsr()), (Wave::Triangle, piano));
	for wave in [Wave::Sine, Wave::Saw, Wave::Triangle] {
		timbre.set_wave(wave);
		assert_eq!(timbre.wave(), wave);
	}
	timbre.set_adsr(Adsr { attack: 5.0, ..piano });
	assert_eq!(timbre.adsr().attack, Adsr::MAX_TIME, "stored inside the limits");
	// A sustain of nothing: the note dies at the end of its decay, and its voice with it.
	timbre.set_adsr(Adsr {
		attack: 0.01,
		decay: 0.01,
		sustain: 0.0,
		release: 0.01,
	});
	let mut synth = BuiltinSynth::new(SAMPLE_RATE, Arc::new(timbre));
	synth.note_on(0, A4, FULL);
	let plucked = render(&mut synth, SECOND / 10);
	assert!(peak(&plucked) > 0.05);
	assert!(is_silent(&plucked[SECOND * 3 / 100..]));
}

/// Renders `frames` frames and returns both sides; with an effect sending, they differ.
fn render_stereo(synth: &mut BuiltinSynth, frames: usize) -> (Vec<f32>, Vec<f32>) {
	let mut left = vec![9.0; frames];
	let mut right = vec![-9.0; frames];
	synth.render(&mut left, &mut right);
	(left, right)
}

/// Both sides of a rendered stretch.
type Stereo = (Vec<f32>, Vec<f32>);

/// A short organ note on channel 1 with the given send levels: the note itself, then the second after it.
fn note_with_sends(reverb: u8, chorus: u8) -> (Stereo, Stereo) {
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.control_change(0, controller::REVERB_SEND, reverb);
	synth.control_change(0, controller::CHORUS_SEND, chorus);
	synth.note_on(0, A4, FULL);
	let note = render_stereo(&mut synth, SECOND / 5);
	synth.note_off(0, A4);
	let after = render_stereo(&mut synth, SECOND);
	(note, after)
}

#[test]
fn with_both_sends_at_zero_the_signal_is_dry() {
	let ((note_left, note_right), (after_left, after_right)) = note_with_sends(0, 0);
	let (mut plain, _) = synth(Wave::Sine, ORGAN);
	plain.note_on(0, A4, FULL);
	assert_eq!(
		note_left,
		render(&mut plain, SECOND / 5),
		"the same samples as with no send ever set"
	);
	assert_eq!(note_left, note_right);
	// The release is a millisecond long: after it there is nothing, on either side.
	assert!(is_silent(&after_left[SECOND / 100..]) && is_silent(&after_right[SECOND / 100..]));
}

#[test]
fn the_reverb_leaves_a_tail_after_the_note_and_follows_its_send_level() {
	let ((note_left, _), (after_left, after_right)) = note_with_sends(127, 0);
	let (_, (dry_after, _)) = note_with_sends(0, 0);
	assert!(is_silent(&dry_after[SECOND / 100..]), "without the reverb the note is over");
	let tail = |signal: &[f32], from: usize, to: usize| rms(&signal[SECOND * from / 10..SECOND * to / 10]);
	assert!(
		tail(&after_left, 1, 5) > 0.001,
		"the room still sounds: {}",
		tail(&after_left, 1, 5)
	);
	assert!(tail(&after_left, 6, 10) < tail(&after_left, 1, 5) / 2.0, "and fades");
	assert_ne!(after_left, after_right, "the reverb gives the sound its width");
	assert!(peak(&note_left) > VOICE_GAIN, "while the note sounds the reverb adds to it");

	// Half the send, half the tail.
	let (_, (half_left, _)) = note_with_sends(64, 0);
	let share = tail(&half_left, 1, 5) / tail(&after_left, 1, 5);
	assert_close(share, 64.0 / 127.0, 0.01, "send 64 against send 127");
}

#[test]
fn the_chorus_doubles_the_note_with_a_swaying_copy_and_ends_with_it() {
	let ((note_left, note_right), (after_left, after_right)) = note_with_sends(0, 127);
	let ((dry, _), _) = note_with_sends(0, 0);
	assert_ne!(note_left, dry);
	assert_ne!(note_left, note_right, "the copies of the two sides are delayed differently");
	// The copy is as loud as the note: together they reach up to twice its peak, never more.
	let together = peak(&note_left).max(peak(&note_right));
	assert!(together > VOICE_GAIN * 1.2 && together <= VOICE_GAIN * 2.001, "{together}");
	// The longest delay is 3.9 ms: ten milliseconds after the note there is nothing left of it.
	assert!(
		peak(&after_left[..SECOND / 500]) > 0.01,
		"the copy is still arriving right after the note"
	);
	assert!(is_silent(&after_left[SECOND / 100..]) && is_silent(&after_right[SECOND / 100..]));

	// Half the send, a copy half as loud.
	let ((half_left, _), _) = note_with_sends(0, 64);
	let added = |signal: &[f32]| rms(&signal.iter().zip(&dry).map(|(wet, dry)| wet - dry).collect::<Vec<f32>>());
	assert_close(
		added(&half_left) / added(&note_left),
		64.0 / 127.0,
		0.01,
		"send 64 against send 127",
	);
}

#[test]
fn a_send_belongs_to_its_channel() {
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.control_change(0, controller::REVERB_SEND, 127);
	synth.control_change(0, controller::CHORUS_SEND, 127);
	// A note on another channel is not sent anywhere.
	synth.note_on(1, A4, FULL);
	let (left, right) = render_stereo(&mut synth, SECOND / 5);
	let (mut plain, _) = self::synth(Wave::Sine, ORGAN);
	plain.note_on(1, A4, FULL);
	assert_eq!(left, render(&mut plain, SECOND / 5));
	assert_eq!(left, right);
}

#[test]
fn a_long_loud_chord_with_both_effects_stays_within_bounds_and_ends_in_silence() {
	let (mut synth, _) = synth(Wave::Saw, ORGAN);
	for channel in 0..16 {
		synth.control_change(channel, controller::REVERB_SEND, 127);
		synth.control_change(channel, controller::CHORUS_SEND, 127);
	}
	let chord: [u8; 8] = [45, 52, 57, 60, 64, 69, 72, 76];
	for (channel, key) in (0u8..).zip(chord) {
		synth.note_on(channel, key, FULL);
	}
	// Eight full voices peak at 1.6 dry; the chorus can double that, the reverb adds its room.
	let mut loudest = 0.0f32;
	for _ in 0..20 {
		let (left, right) = render_stereo(&mut synth, SECOND);
		assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
		loudest = loudest.max(peak(&left)).max(peak(&right));
	}
	assert!(
		loudest > 1.0 && loudest < 6.0,
		"twenty seconds on, the level has not built up: {loudest}"
	);

	for (channel, key) in (0u8..).zip(chord) {
		synth.note_off(channel, key);
	}
	let mut seconds = 0;
	loop {
		let (left, right) = render_stereo(&mut synth, SECOND);
		seconds += 1;
		if is_silent(&left) && is_silent(&right) {
			break;
		}
		assert!(seconds < 60, "the tail has not ended after a minute");
	}
}

#[test]
fn buffers_of_different_lengths_are_rendered_up_to_the_shorter_one() {
	let (mut synth, _) = synth(Wave::Sine, ORGAN);
	synth.note_on(0, A4, FULL);
	let mut left = vec![9.0; 300];
	let mut right = vec![9.0; 200];
	synth.render(&mut left, &mut right);
	assert_eq!(left[..200], right[..]);
	assert!(peak(&right) > 0.1);
	assert!(is_silent(&left[200..]), "what one buffer has beyond the other is silence");
}
