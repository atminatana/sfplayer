//! Level meter presentation (spec F-09): dBFS text, the share of the bar to fill and the one-second clip hold.

use std::time::{Duration, Instant};

use crate::audio::meter::{DBFS_FLOOR, to_dbfs};

/// Spec F-09: the CLIP mark stays lit this long after the last clipped sample.
pub const CLIP_HOLD: Duration = Duration::from_secs(1);

/// Remembers the clip counter published by the audio thread and when it last grew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipHold {
	last_count: u32,
	lit_until: Option<Instant>,
}

impl ClipHold {
	pub fn new(initial_count: u32) -> Self {
		Self {
			last_count: initial_count,
			lit_until: None,
		}
	}

	/// Feeds the current clip counter; a change re-arms the hold timer.
	pub fn update(&mut self, count: u32, now: Instant) {
		if count != self.last_count {
			self.last_count = count;
			self.lit_until = Some(now + CLIP_HOLD);
		}
	}

	pub fn is_lit(&self, now: Instant) -> bool {
		self.lit_until.is_some_and(|until| now < until)
	}
}

/// Lowest level drawn on the bar; below it the bar is empty.
pub const BAR_FLOOR_DB: f32 = -60.0;

/// Share of a level bar to fill, 0..=1: linear in decibels between `BAR_FLOOR_DB` and 0 dBFS.
pub fn level_ratio(level: f32) -> f64 {
	let db = to_dbfs(level).unwrap_or(DBFS_FLOOR).clamp(BAR_FLOOR_DB, 0.0);
	f64::from((db - BAR_FLOOR_DB) / -BAR_FLOOR_DB)
}

/// "-6.2 dBFS" or the silence symbol; width is fixed so columns stay aligned.
pub fn dbfs_text(level: f32, silence: &str) -> String {
	match to_dbfs(level) {
		Some(db) => format!("{db:>6.1} dBFS"),
		None => format!("{silence:>6} dBFS"),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn clip_mark_holds_for_one_second_after_the_last_clip() {
		let start = Instant::now();
		let mut hold = ClipHold::new(0);
		assert!(!hold.is_lit(start));
		hold.update(0, start);
		assert!(!hold.is_lit(start), "unchanged counter does not light the mark");
		hold.update(3, start);
		assert!(hold.is_lit(start + Duration::from_millis(999)));
		assert!(!hold.is_lit(start + Duration::from_millis(1001)));
		hold.update(4, start + Duration::from_millis(800));
		assert!(hold.is_lit(start + Duration::from_millis(1700)), "a new clip extends the hold");
	}

	#[test]
	fn bar_share_is_linear_in_decibels_between_the_floor_and_full_scale() {
		assert_eq!(level_ratio(1.0), 1.0);
		assert_eq!(level_ratio(0.0), 0.0);
		assert_eq!(level_ratio(0.001), 0.0, "-60 dBFS is the empty end");
		assert!((level_ratio(0.5) - 0.9).abs() < 0.001, "-6 dB of 60 is 90 %");
		assert_eq!(level_ratio(2.0), 1.0, "clamped at full scale");
		assert_eq!(level_ratio(0.000_001), 0.0, "below the floor the bar is empty");
	}

	#[test]
	fn dbfs_text_is_fixed_width() {
		assert_eq!(dbfs_text(1.0, "-inf"), "   0.0 dBFS");
		assert_eq!(dbfs_text(0.5, "-inf"), "  -6.0 dBFS");
		assert_eq!(dbfs_text(0.0, "-inf"), "  -inf dBFS");
		assert_eq!(dbfs_text(0.0, "−∞").chars().count(), "  -inf dBFS".chars().count());
	}
}
