//! Output level measurement (spec F-09): peak and RMS per channel plus clipped-sample count, computed per block
//! inside the audio callback (plain arithmetic, no allocation) and published through atomics.

/// Linear levels of one rendered block, after the master gain.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Levels {
	pub peak_left: f32,
	pub peak_right: f32,
	pub rms_left: f32,
	pub rms_right: f32,
}

/// Measures `left`/`right` as they will sound after `gain`; returns the levels and how many samples clip (|x| ≥ 1).
pub fn measure(left: &[f32], right: &[f32], gain: f32) -> (Levels, u32) {
	let (peak_left, rms_left, clips_left) = channel_stats(left, gain);
	let (peak_right, rms_right, clips_right) = channel_stats(right, gain);
	(
		Levels {
			peak_left,
			peak_right,
			rms_left,
			rms_right,
		},
		clips_left + clips_right,
	)
}

fn channel_stats(samples: &[f32], gain: f32) -> (f32, f32, u32) {
	if samples.is_empty() {
		return (0.0, 0.0, 0);
	}
	let mut peak = 0.0f32;
	let mut sum_squares = 0.0f32;
	let mut clips = 0u32;
	for sample in samples {
		let value = (sample * gain).abs();
		peak = peak.max(value);
		sum_squares += value * value;
		if value >= 1.0 {
			clips += 1;
		}
	}
	(peak, (sum_squares / samples.len() as f32).sqrt(), clips)
}

/// Lowest level the meter distinguishes; anything quieter is shown as silence.
pub const DBFS_FLOOR: f32 = -90.0;

/// Converts a linear level to dBFS; `None` means silence (below the floor).
pub fn to_dbfs(level: f32) -> Option<f32> {
	if level <= 0.0 {
		return None;
	}
	let db = 20.0 * level.log10();
	(db > DBFS_FLOOR).then_some(db)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sine(amplitude: f32, len: usize) -> Vec<f32> {
		(0..len)
			.map(|i| amplitude * (i as f32 / len as f32 * std::f32::consts::TAU * 4.0).sin())
			.collect()
	}

	#[test]
	fn sine_has_known_peak_and_rms() {
		let left = sine(0.5, 4800);
		let right = sine(0.25, 4800);
		let (levels, clips) = measure(&left, &right, 1.0);
		assert!((levels.peak_left - 0.5).abs() < 0.001, "{}", levels.peak_left);
		assert!(
			(levels.rms_left - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.002,
			"{}",
			levels.rms_left
		);
		assert!((levels.peak_right - 0.25).abs() < 0.001);
		assert_eq!(clips, 0);
	}

	#[test]
	fn gain_is_applied_before_measuring() {
		let signal = sine(0.5, 480);
		let (levels, _) = measure(&signal, &signal, 0.5);
		assert!((levels.peak_left - 0.25).abs() < 0.001);
		let (muted, _) = measure(&signal, &signal, 0.0);
		assert_eq!(muted, Levels::default());
	}

	#[test]
	fn clipped_samples_are_counted_on_both_channels() {
		let left = vec![1.0, -1.2, 0.3];
		let right = vec![0.0, 0.0, 2.0];
		let (_, clips) = measure(&left, &right, 1.0);
		assert_eq!(clips, 3);
		let (_, clips) = measure(&left, &right, 0.5);
		assert_eq!(clips, 1, "only 2.0 × 0.5 reaches full scale");
	}

	#[test]
	fn empty_block_is_silent() {
		assert_eq!(measure(&[], &[], 1.0), (Levels::default(), 0));
	}

	#[test]
	fn dbfs_conversion() {
		assert_eq!(to_dbfs(1.0), Some(0.0));
		assert!((to_dbfs(0.5).unwrap() + 6.02).abs() < 0.01);
		assert_eq!(to_dbfs(0.0), None);
		assert_eq!(to_dbfs(1e-6), None, "below the floor counts as silence");
	}
}
