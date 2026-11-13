//! Offline tests of the two effects on their own: what comes out for a known input.

use super::chorus::Chorus;
use super::reverb::Reverb;
use super::*;
use crate::synth::test_support::rms;

const SAMPLE_RATE: u32 = 48_000;
const SECOND: usize = SAMPLE_RATE as usize;

/// A single sample of 1 followed by silence.
fn impulse(frames: usize) -> Vec<f32> {
	let mut signal = vec![0.0; frames];
	signal[0] = 1.0;
	signal
}

fn reverberated(reverb: &mut Reverb, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
	let mut left = vec![9.0; input.len()];
	let mut right = vec![9.0; input.len()];
	reverb.process(input, &mut left, &mut right);
	(left, right)
}

fn chorused(chorus: &mut Chorus, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
	let mut left = vec![9.0; input.len()];
	let mut right = vec![9.0; input.len()];
	chorus.process(input, input, &mut left, &mut right);
	(left, right)
}

fn rustysynth_chorus(sample_rate: u32) -> Chorus {
	Chorus::new(sample_rate, CHORUS_DELAY_SECONDS, CHORUS_DEPTH_SECONDS, CHORUS_FREQUENCY_HZ)
}

fn first_sound(signal: &[f32]) -> Option<usize> {
	signal.iter().position(|sample| *sample != 0.0)
}

fn is_silent(signal: &[f32]) -> bool {
	signal.iter().all(|sample| *sample == 0.0)
}

#[test]
fn the_reverb_of_silence_is_silence() {
	let mut reverb = Reverb::new(SAMPLE_RATE);
	let (left, right) = reverberated(&mut reverb, &vec![0.0; SECOND]);
	assert!(is_silent(&left) && is_silent(&right));
}

#[test]
fn the_first_echo_comes_after_the_shortest_delay_line_and_later_on_the_right() {
	// At the reference rate the tunings are the delays themselves: 1116 samples, and 23 more on the right.
	let mut reverb = Reverb::new(44_100);
	let (left, right) = reverberated(&mut reverb, &impulse(4_000));
	assert_eq!(first_sound(&left), Some(1116));
	assert_eq!(first_sound(&right), Some(1116 + 23));
	// At another rate the delays keep their length in time.
	let mut reverb = Reverb::new(SAMPLE_RATE);
	let (left, right) = reverberated(&mut reverb, &impulse(4_000));
	assert_eq!(first_sound(&left), Some(1215), "1116 samples of 44.1 kHz at 48 kHz");
	assert_eq!(first_sound(&right), Some(1240), "1139 samples of 44.1 kHz at 48 kHz");
}

#[test]
fn the_first_echoes_of_a_click_are_what_the_filters_make_of_it() {
	// At the reference rate, on the left side. The comb filters return the click after 1116, 1188 and
	// 1277 samples; each of the four all-pass filters turns a first arrival upside down, and four
	// times upside down is the right way up.
	let mut reverb = Reverb::new(44_100);
	let (left, right) = reverberated(&mut reverb, &impulse(2_000));
	for echo in [1116, 1188, 1277] {
		assert_eq!(left[echo], 1.0, "the echo of the comb filter {echo} samples long");
	}
	assert!(is_silent(&left[1117..1188]), "nothing between the echoes yet");
	// The last all-pass filter is 225 samples long: what it took in comes back out once, whole and
	// upside down, and then again at half the level — its feedback.
	assert_eq!(left[1116 + 225], -1.0);
	assert_eq!(left[1116 + 2 * 225], -0.5);
	// The right side is built the same way, every delay line 23 samples longer.
	assert_eq!(right[1116 + 23], 1.0);
	assert_eq!(right[1116 + 23 + 225 + 23], -1.0);
	assert_eq!(right[1116 + 23 + 2 * (225 + 23)], -0.5);
}

#[test]
fn a_low_note_fades_at_the_rate_the_room_size_sets() {
	// On every pass through a comb filter a low tone keeps 0.84 of its level — the room size; the
	// damping takes nothing from it. The filters are 1215 to 1760 samples long at this rate, so in
	// a second the tone passes them 27 to 39 times: between 0.84^39 and 0.84^27 of it is left,
	// 0.001 to 0.009.
	let mut reverb = Reverb::new(SAMPLE_RATE);
	let burst: Vec<f32> = (0..SECOND / 5)
		.map(|frame| (std::f32::consts::TAU * 200.0 * frame as f32 / SAMPLE_RATE as f32).sin())
		.collect();
	reverberated(&mut reverb, &burst);
	let (tail, _) = reverberated(&mut reverb, &vec![0.0; SECOND * 5 / 4]);
	let left_after_a_second = rms(&tail[SECOND..]) / rms(&tail[..SECOND / 4]);
	assert!(
		left_after_a_second > 0.001 && left_after_a_second < 0.009,
		"{left_after_a_second} of the level is left a second on"
	);
}

#[test]
fn the_room_swallows_high_notes_sooner_than_low_ones() {
	let tail_of = |hertz: usize| {
		let mut reverb = Reverb::new(SAMPLE_RATE);
		let burst: Vec<f32> = (0..SECOND / 5)
			.map(|frame| (std::f32::consts::TAU * hertz as f32 * frame as f32 / SAMPLE_RATE as f32).sin() * 0.03)
			.collect();
		reverberated(&mut reverb, &burst);
		let (left, _) = reverberated(&mut reverb, &vec![0.0; SECOND]);
		rms(&left[SECOND / 2..])
	};
	let (low, high) = (tail_of(200), tail_of(8_000));
	assert!(low > 0.0005, "the low note still rings half a second on: {low}");
	assert!(high < low / 4.0, "the high one is damped away: {high} against {low}");
}

#[test]
fn a_click_leaves_a_tail_that_fades_and_ends() {
	let mut reverb = Reverb::new(SAMPLE_RATE);
	let (left, right) = reverberated(&mut reverb, &impulse(3 * SECOND));
	let (early, middle, late) = (
		rms(&left[SECOND / 10..SECOND / 2]),
		rms(&left[SECOND..SECOND * 3 / 2]),
		rms(&left[SECOND * 5 / 2..]),
	);
	assert!(early > 0.01, "the tail is there: {early}");
	assert!(
		middle > 0.0 && middle < early / 2.0,
		"a second on it has faded: {middle} of {early}"
	);
	assert!(late < middle / 2.0, "and goes on fading: {late} of {middle}");
	assert_ne!(left, right, "the two sides are tuned apart");

	// Fed nothing more, the tail dies down to exact silence instead of ringing on in tiny numbers.
	let silence = vec![0.0; SECOND];
	let mut seconds = 0;
	loop {
		let (left, right) = reverberated(&mut reverb, &silence);
		seconds += 1;
		if is_silent(&left) && is_silent(&right) {
			break;
		}
		assert!(seconds < 60, "the tail has not ended after a minute");
	}
}

#[test]
fn the_reverb_stays_within_bounds_on_a_long_loud_signal() {
	let mut reverb = Reverb::new(SAMPLE_RATE);
	// A full-scale square wave for twenty seconds, at the gain a full send gives it.
	let square: Vec<f32> = (0..SECOND)
		.map(|frame| {
			if (frame / 109) % 2 == 0 {
				Effects::REVERB_SEND_GAIN
			} else {
				-Effects::REVERB_SEND_GAIN
			}
		})
		.collect();
	let mut loudest = 0.0f32;
	for _ in 0..20 {
		let (left, right) = reverberated(&mut reverb, &square);
		assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
		loudest = left.iter().chain(&right).fold(loudest, |peak, sample| peak.max(sample.abs()));
	}
	assert!(loudest > 0.05, "it does reverberate: {loudest}");
	assert!(loudest < 2.0, "and does not build up: {loudest}");
}

#[test]
fn the_reverb_does_not_depend_on_how_the_signal_is_cut_into_blocks() {
	let signal: Vec<f32> = (0..5_000).map(|frame| ((frame * 7919) % 200) as f32 / 100.0 - 1.0).collect();
	let whole = reverberated(&mut Reverb::new(SAMPLE_RATE), &signal);
	let mut reverb = Reverb::new(SAMPLE_RATE);
	let (mut left, mut right) = (Vec::new(), Vec::new());
	let mut rest = &signal[..];
	for size in [1, 7, 64, 256, 1300, 3372] {
		let (block, tail) = rest.split_at(size);
		let (block_left, block_right) = reverberated(&mut reverb, block);
		left.extend(block_left);
		right.extend(block_right);
		rest = tail;
	}
	assert!(rest.is_empty());
	assert_eq!((left, right), whole);
}

#[test]
fn the_chorus_of_silence_is_silence() {
	let (left, right) = chorused(&mut rustysynth_chorus(SAMPLE_RATE), &vec![0.0; SECOND]);
	assert!(is_silent(&left) && is_silent(&right));
}

#[test]
fn the_chorus_returns_the_signal_after_its_swaying_delay() {
	// The sway starts in the middle on the left (2 ms) and at its longest on the right (3.9 ms).
	let (left, right) = chorused(&mut rustysynth_chorus(SAMPLE_RATE), &impulse(1_000));
	let loudest = |signal: &[f32]| (0..signal.len()).max_by(|a, b| signal[*a].total_cmp(&signal[*b])).expect("frames");
	assert!(
		(95..=97).contains(&loudest(&left)),
		"left after about 96 samples: {}",
		loudest(&left)
	);
	assert!(
		(186..=188).contains(&loudest(&right)),
		"right after about 187 samples: {}",
		loudest(&right)
	);
	// A delay of 187.2 samples falls between two samples: the click is shared 0.8 to 0.2.
	assert!(
		(right[187] - 0.8).abs() < 0.01 && (right[188] - 0.2).abs() < 0.01,
		"{} {}",
		right[187],
		right[188]
	);
	// Nothing but the one delayed copy: the click comes back whole, shared between two samples at most.
	for (name, side) in [("left", &left), ("right", &right)] {
		let total: f32 = side.iter().sum();
		assert!((total - 1.0).abs() < 0.02, "{name}: {total}");
		assert!(side.iter().filter(|sample| **sample != 0.0).count() <= 3, "{name}");
	}

	// A quarter of the sway later (0.625 s) the left side is where the right side started.
	let mut chorus = rustysynth_chorus(SAMPLE_RATE);
	chorused(&mut chorus, &vec![0.0; SECOND * 5 / 8]);
	let (left, right) = chorused(&mut chorus, &impulse(1_000));
	assert!((186..=188).contains(&loudest(&left)), "left: {}", loudest(&left));
	assert!(
		(95..=97).contains(&loudest(&right)),
		"right, half a sway from its start: {}",
		loudest(&right)
	);
}

#[test]
fn the_chorus_does_not_depend_on_how_the_signal_is_cut_into_blocks() {
	let signal: Vec<f32> = (0..3_000).map(|frame| ((frame * 7919) % 200) as f32 / 100.0 - 1.0).collect();
	let whole = chorused(&mut rustysynth_chorus(SAMPLE_RATE), &signal);
	let mut chorus = rustysynth_chorus(SAMPLE_RATE);
	let (mut left, mut right) = (Vec::new(), Vec::new());
	let mut rest = &signal[..];
	for size in [1, 7, 64, 256, 2672] {
		let (block, tail) = rest.split_at(size);
		let (block_left, block_right) = chorused(&mut chorus, block);
		left.extend(block_left);
		right.extend(block_right);
		rest = tail;
	}
	assert!(rest.is_empty());
	assert_eq!((left, right), whole);
}

#[test]
fn the_settings_are_those_of_rustysynth() {
	// Its reverb takes 0.015 of each side of a voice, and a voice of this synthesizer is on both.
	assert_eq!(Effects::REVERB_SEND_GAIN, 0.03);
	assert_eq!(
		(CHORUS_DELAY_SECONDS, CHORUS_DEPTH_SECONDS, CHORUS_FREQUENCY_HZ),
		(0.002, 0.0019, 0.4)
	);
}

#[test]
fn a_block_adds_what_the_effects_make_of_the_sends_to_both_sides() {
	let mut effects = Effects::new(SAMPLE_RATE);
	// Nothing sent: the two sides come back as they went in.
	let (reverb_input, chorus_input) = effects.begin_block(Effects::BLOCK);
	assert_eq!((reverb_input.len(), chorus_input.len()), (Effects::BLOCK, Effects::BLOCK));
	let mut left = vec![0.25; Effects::BLOCK];
	let mut right = vec![-0.25; Effects::BLOCK];
	effects.finish_block(&mut left, &mut right);
	assert!(left.iter().all(|sample| *sample == 0.25) && right.iter().all(|sample| *sample == -0.25));

	// A click sent to the chorus comes back on top of what the sides hold, 96 samples on.
	let (_, chorus_input) = effects.begin_block(Effects::BLOCK);
	chorus_input[0] = 1.0;
	let mut left = vec![0.25; Effects::BLOCK];
	let mut right = vec![0.25; Effects::BLOCK];
	effects.finish_block(&mut left, &mut right);
	assert!(left[..90].iter().all(|sample| *sample == 0.25));
	let added: f32 = left.iter().map(|sample| sample - 0.25).sum();
	assert!((added - 1.0).abs() < 0.02, "{added}");

	// A click sent to the reverb comes back as well: its first echo is 1215 samples away, the fifth block on.
	let mut effects = Effects::new(SAMPLE_RATE);
	let (reverb_input, _) = effects.begin_block(Effects::BLOCK);
	reverb_input[0] = 1.0;
	let mut heard = Vec::new();
	for _ in 0..5 {
		let mut left = vec![0.0; Effects::BLOCK];
		let mut right = vec![0.0; Effects::BLOCK];
		effects.finish_block(&mut left, &mut right);
		heard.extend(left);
		effects.begin_block(Effects::BLOCK);
	}
	assert_eq!(first_sound(&heard), Some(1215));
	assert_eq!(heard[1215], 1.0);

	// The send buffers of the next block start cleared, and a shorter block is as long as asked.
	let (reverb_input, chorus_input) = effects.begin_block(10);
	assert_eq!((reverb_input.len(), chorus_input.len()), (10, 10));
	assert!(is_silent(reverb_input) && is_silent(chorus_input));
	// A block longer than the limit is cut to the limit.
	let (reverb_input, _) = effects.begin_block(Effects::BLOCK + 100);
	assert_eq!(reverb_input.len(), Effects::BLOCK);
}
