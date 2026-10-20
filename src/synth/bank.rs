//! Loading a SoundFont 2 file and listing its presets (spec F-05, F-06).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustysynth::{SoundFont, SoundFontError, SynthesizerError};

#[derive(Debug, thiserror::Error)]
pub enum SynthError {
	#[error("cannot open SoundFont file {path}")]
	Open {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("{path} is not a valid SoundFont 2 file")]
	InvalidSoundFont {
		path: PathBuf,
		#[source]
		source: SoundFontError,
	},
	#[error("cannot create synthesizer")]
	Synthesizer(#[from] SynthesizerError),
}

/// One selectable preset: MIDI bank and program numbers plus the name stored in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetInfo {
	pub bank: u16,
	pub program: u8,
	pub name: String,
}

/// A loaded SoundFont shared (read-only) between the UI and the audio engine.
pub struct SoundFontBank {
	font: Arc<SoundFont>,
	name: String,
	presets: Vec<PresetInfo>,
}

impl SoundFontBank {
	pub fn load(path: &Path) -> Result<Self, SynthError> {
		let file = File::open(path).map_err(|source| SynthError::Open {
			path: path.to_owned(),
			source,
		})?;
		let mut reader = BufReader::new(file);
		let font = SoundFont::new(&mut reader).map_err(|source| SynthError::InvalidSoundFont {
			path: path.to_owned(),
			source,
		})?;
		let name = bank_name(&font, path);
		let presets = list_presets(&font);
		Ok(Self {
			font: Arc::new(font),
			name,
			presets,
		})
	}

	/// Bank name from the file's INFO chunk, or the file stem when the chunk is empty.
	pub fn name(&self) -> &str {
		&self.name
	}

	/// Presets sorted by bank, then program.
	pub fn presets(&self) -> &[PresetInfo] {
		&self.presets
	}

	pub fn font(&self) -> &Arc<SoundFont> {
		&self.font
	}
}

fn bank_name(font: &SoundFont, path: &Path) -> String {
	let stored = font.get_info().get_bank_name().trim();
	if !stored.is_empty() {
		return stored.to_owned();
	}
	path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

fn list_presets(font: &SoundFont) -> Vec<PresetInfo> {
	let mut presets: Vec<PresetInfo> = font
		.get_presets()
		.iter()
		.map(|preset| PresetInfo {
			bank: u16::try_from(preset.get_bank_number()).unwrap_or(u16::MAX),
			program: u8::try_from(preset.get_patch_number()).unwrap_or(u8::MAX),
			name: preset.get_name().trim().to_owned(),
		})
		.collect();
	presets.sort_by_key(|preset| (preset.bank, preset.program));
	presets
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::synth::test_support::TEST_BANK_PATH;

	const MISSING_PATH: &str = "assets/does-not-exist.sf2";

	#[test]
	fn loads_test_bank_with_presets() {
		let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
		assert!(!bank.presets().is_empty());
		assert_eq!(bank.presets()[0].name, "Sine Wave");
		assert_eq!(bank.presets()[0].bank, 0);
	}

	#[test]
	fn bank_name_comes_from_info_chunk() {
		let bank = SoundFontBank::load(Path::new(TEST_BANK_PATH)).expect("test bank loads");
		assert_eq!(bank.name(), "User Bank");
	}

	#[test]
	fn missing_file_is_open_error() {
		let error = SoundFontBank::load(Path::new(MISSING_PATH)).err().expect("missing file fails");
		assert!(matches!(error, SynthError::Open { .. }), "{error:?}");
	}

	#[test]
	fn non_sf2_file_is_invalid_error() {
		let not_a_bank = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
		let error = SoundFontBank::load(not_a_bank).err().expect("text file fails");
		assert!(matches!(error, SynthError::InvalidSoundFont { .. }), "{error:?}");
	}
}
