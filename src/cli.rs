//! Command-line interface (spec §5, "Командная строка").

use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;
use tracing::level_filters::LevelFilter;

/// Default audio buffer size in frames (spec F-04).
pub const DEFAULT_BUFFER_FRAMES: u32 = 256;

#[derive(Parser, Debug)]
#[command(
	name = "sf2_player",
	version,
	about = "Console SoundFont synthesizer: MIDI in, SF2 bank, audio out"
)]
pub struct Cli {
	/// Path to the SoundFont (.sf2) bank
	#[arg(long, value_name = "PATH")]
	pub sf2: Option<PathBuf>,

	/// MIDI input port: name substring or index from --list-devices
	#[arg(long, value_name = "NAME|INDEX")]
	pub midi_in: Option<DeviceSelector>,

	/// Audio output device: name substring or index from --list-devices
	#[arg(long, value_name = "NAME|INDEX")]
	pub audio_out: Option<DeviceSelector>,

	/// Audio buffer size in frames
	#[arg(long, value_name = "FRAMES", default_value_t = DEFAULT_BUFFER_FRAMES, value_parser = clap::value_parser!(u32).range(1..))]
	pub buffer: u32,

	/// Sample rate in Hz (default: the device's own rate)
	#[arg(long, value_name = "HZ", value_parser = clap::value_parser!(u32).range(1..))]
	pub sample_rate: Option<u32>,

	/// Print MIDI inputs and audio outputs, then exit without starting the TUI
	#[arg(long)]
	pub list_devices: bool,

	/// Path to the TOML config file (default: the OS config directory)
	#[arg(long, value_name = "PATH")]
	pub config: Option<PathBuf>,

	/// Log level for the log file: trace, debug, info, warn, error, off
	#[arg(long, value_name = "LEVEL", default_value = "info")]
	pub log_level: LevelFilter,
}

/// How the user refers to a MIDI port or audio device: by list index or by a name substring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelector {
	Index(usize),
	Name(String),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("device selector must be a non-empty name or an index")]
pub struct EmptyDeviceSelector;

impl DeviceSelector {
	/// Resolves the selector against an enumeration: an index must exist; a name matches the first
	/// entry containing it, case-insensitively.
	pub fn find<'a>(&self, mut names: impl Iterator<Item = &'a str>) -> Option<usize> {
		match self {
			Self::Index(index) => names
				.enumerate()
				.find(|(position, _)| position == index)
				.map(|(position, _)| position),
			Self::Name(needle) => {
				let needle = needle.to_lowercase();
				names.position(|name| name.to_lowercase().contains(&needle))
			}
		}
	}
}

impl FromStr for DeviceSelector {
	type Err = EmptyDeviceSelector;

	fn from_str(raw: &str) -> Result<Self, Self::Err> {
		let trimmed = raw.trim();
		if trimmed.is_empty() {
			return Err(EmptyDeviceSelector);
		}
		Ok(match trimmed.parse::<usize>() {
			Ok(index) => Self::Index(index),
			Err(_) => Self::Name(trimmed.to_owned()),
		})
	}
}

#[cfg(test)]
mod tests {
	use clap::CommandFactory;

	use super::*;

	const BIN: &str = "sf2_player";

	#[test]
	fn cli_definition_is_consistent() {
		Cli::command().debug_assert();
	}

	#[test]
	fn defaults_match_spec() {
		let cli = Cli::parse_from([BIN]);
		assert_eq!(cli.buffer, DEFAULT_BUFFER_FRAMES);
		assert!(cli.sample_rate.is_none());
		assert!(cli.sf2.is_none());
		assert!(cli.midi_in.is_none());
		assert!(cli.audio_out.is_none());
		assert!(cli.config.is_none());
		assert!(!cli.list_devices);
		assert_eq!(cli.log_level, LevelFilter::INFO);
	}

	#[test]
	fn all_options_parse() {
		let cli = Cli::parse_from([
			BIN,
			"--sf2",
			"bank.sf2",
			"--midi-in",
			"Arturia",
			"--audio-out",
			"2",
			"--buffer",
			"512",
			"--sample-rate",
			"44100",
			"--list-devices",
			"--config",
			"my.toml",
			"--log-level",
			"debug",
		]);
		assert_eq!(cli.sf2.as_deref(), Some(std::path::Path::new("bank.sf2")));
		assert_eq!(cli.midi_in, Some(DeviceSelector::Name("Arturia".into())));
		assert_eq!(cli.audio_out, Some(DeviceSelector::Index(2)));
		assert_eq!(cli.buffer, 512);
		assert_eq!(cli.sample_rate, Some(44_100));
		assert!(cli.list_devices);
		assert_eq!(cli.config.as_deref(), Some(std::path::Path::new("my.toml")));
		assert_eq!(cli.log_level, LevelFilter::DEBUG);
	}

	#[test]
	fn zero_buffer_is_rejected() {
		assert!(Cli::try_parse_from([BIN, "--buffer", "0"]).is_err());
	}

	#[test]
	fn unknown_log_level_is_rejected() {
		assert!(Cli::try_parse_from([BIN, "--log-level", "loud"]).is_err());
	}

	#[test]
	fn selector_digits_become_index() {
		assert_eq!("3".parse(), Ok(DeviceSelector::Index(3)));
		assert_eq!(" 12 ".parse(), Ok(DeviceSelector::Index(12)));
	}

	#[test]
	fn selector_text_becomes_name() {
		assert_eq!("Speakers".parse(), Ok(DeviceSelector::Name("Speakers".into())));
		assert_eq!("2 - Headphones".parse(), Ok(DeviceSelector::Name("2 - Headphones".into())));
	}

	#[test]
	fn selector_blank_is_error() {
		assert_eq!("".parse::<DeviceSelector>(), Err(EmptyDeviceSelector));
		assert_eq!("   ".parse::<DeviceSelector>(), Err(EmptyDeviceSelector));
	}

	#[test]
	fn find_by_index_requires_existing_position() {
		let names = ["Arturia KeyStep", "loopMIDI Port"];
		assert_eq!(DeviceSelector::Index(1).find(names.iter().copied()), Some(1));
		assert_eq!(DeviceSelector::Index(2).find(names.iter().copied()), None);
	}

	#[test]
	fn find_by_name_is_case_insensitive_substring() {
		let names = ["Arturia KeyStep", "loopMIDI Port"];
		assert_eq!(DeviceSelector::Name("keystep".into()).find(names.iter().copied()), Some(0));
		assert_eq!(DeviceSelector::Name("LOOP".into()).find(names.iter().copied()), Some(1));
		assert_eq!(DeviceSelector::Name("Roland".into()).find(names.iter().copied()), None);
	}

	#[test]
	fn find_on_empty_list_is_none() {
		assert_eq!(DeviceSelector::Index(0).find(std::iter::empty()), None);
		assert_eq!(DeviceSelector::Name("x".into()).find(std::iter::empty()), None);
	}
}
