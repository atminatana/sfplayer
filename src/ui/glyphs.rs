//! Indicator glyphs (spec §9): Unicode blocks by default, plain ASCII with `--ascii` or when the terminal
//! is unlikely to render them.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
	/// Filled bar cell.
	pub full: &'static str,
	/// Empty bar cell.
	pub empty: &'static str,
	/// Status dot (connected MIDI, current item, REC).
	pub dot: &'static str,
	/// Meter bar brackets.
	pub bar_open: &'static str,
	pub bar_close: &'static str,
}

impl Glyphs {
	pub const UNICODE: Self = Self {
		full: "█",
		empty: "░",
		dot: "●",
		bar_open: "▐",
		bar_close: "▌",
	};
	pub const ASCII: Self = Self {
		full: "#",
		empty: "-",
		dot: "*",
		bar_open: "[",
		bar_close: "]",
	};

	pub fn select(ascii: bool) -> Self {
		if ascii { Self::ASCII } else { Self::UNICODE }
	}

	/// `full`/`empty` repeated to a bar of `width` cells with `filled` of them lit.
	pub fn bar(&self, filled: usize, width: usize) -> String {
		let filled = filled.min(width);
		format!("{}{}", self.full.repeat(filled), self.empty.repeat(width - filled))
	}
}

/// Heuristic: the Windows legacy console (no `WT_SESSION`, no `TERM_PROGRAM`) and `TERM` values `linux` /
/// `dumb` cannot be trusted with block characters; everything else is assumed to be UTF-8 capable.
pub fn terminal_prefers_ascii(env: impl Fn(&str) -> Option<String>) -> bool {
	if matches!(env("TERM").as_deref(), Some("linux" | "dumb")) {
		return true;
	}
	if cfg!(windows) {
		return env("WT_SESSION").is_none() && env("TERM_PROGRAM").is_none() && env("TERM").is_none();
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bars_and_glyph_sets() {
		assert_eq!(Glyphs::UNICODE.bar(3, 5), "███░░");
		assert_eq!(Glyphs::ASCII.bar(3, 5), "###--");
		assert_eq!(Glyphs::ASCII.bar(9, 5), "#####", "clamped");
		assert_eq!(Glyphs::select(true), Glyphs::ASCII);
		assert_eq!(Glyphs::select(false), Glyphs::UNICODE);
	}

	#[test]
	fn detection_flags_dumb_terminals_only() {
		let env = |vars: &'static [(&'static str, &'static str)]| {
			move |name: &str| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_owned())
		};
		assert!(terminal_prefers_ascii(env(&[("TERM", "dumb")])));
		assert!(terminal_prefers_ascii(env(&[("TERM", "linux"), ("WT_SESSION", "x")])));
		assert!(!terminal_prefers_ascii(env(&[("TERM", "xterm-256color")])));
		assert!(!terminal_prefers_ascii(env(&[("WT_SESSION", "abc")])));
		assert_eq!(
			terminal_prefers_ascii(env(&[])),
			cfg!(windows),
			"bare environment: legacy console on Windows only"
		);
	}
}
