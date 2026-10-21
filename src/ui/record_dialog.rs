//! F7 recording settings dialog (spec v1.1 §4.3): a folder field (edited like the F4 path field, Tab completes
//! directories only) and a format row. ↑↓ move between the two, ←→ switch the format, Enter applies,
//! Esc cancels. Pure state; drawing lives in `modals.rs`.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::path_input::{DirEntry, PathInput, PathOutcome};
use crate::config::RecordFormat;

pub const RECORD_FORMATS: [RecordFormat; 2] = [RecordFormat::Flac, RecordFormat::OggVorbis];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordField {
	Directory,
	Format,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
	Pending,
	/// Enter: `dir` is `None` when the field is empty (the OS music folder, as with `record.dir` unset).
	Accepted {
		dir: Option<PathBuf>,
		format: RecordFormat,
	},
	Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDialog {
	directory: PathInput,
	format: RecordFormat,
	focus: RecordField,
	/// `record.vorbis_quality` as shown in the Ogg Vorbis description ("0.5").
	vorbis_quality: String,
}

impl RecordDialog {
	pub fn new(dir: Option<&Path>, format: RecordFormat, vorbis_quality: f32) -> Self {
		let initial = dir.map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_default();
		Self {
			directory: PathInput::new(&initial, Vec::new()),
			format,
			focus: RecordField::Directory,
			vorbis_quality: format!("{vorbis_quality:.1}"),
		}
	}

	pub fn directory(&self) -> &PathInput {
		&self.directory
	}

	pub fn vorbis_quality(&self) -> &str {
		&self.vorbis_quality
	}

	pub fn format(&self) -> RecordFormat {
		self.format
	}

	pub fn focus(&self) -> RecordField {
		self.focus
	}

	/// Terminal paste goes into the folder field whatever the focus.
	pub fn insert_str(&mut self, pasted: &str) {
		self.directory.insert_str(pasted);
	}

	pub fn handle_key(&mut self, key: KeyEvent, list_dir: impl Fn(&Path) -> Vec<DirEntry>) -> RecordOutcome {
		match key.code {
			KeyCode::Esc => return RecordOutcome::Cancelled,
			KeyCode::Enter => {
				let text = self.directory.text().trim().to_owned();
				return RecordOutcome::Accepted {
					dir: (!text.is_empty()).then(|| PathBuf::from(text)),
					format: self.format,
				};
			}
			KeyCode::Up | KeyCode::Down => {
				self.focus = match self.focus {
					RecordField::Directory => RecordField::Format,
					RecordField::Format => RecordField::Directory,
				};
				return RecordOutcome::Pending;
			}
			_ => {}
		}
		match self.focus {
			RecordField::Format => {
				if matches!(key.code, KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')) {
					self.format = match self.format {
						RecordFormat::Flac => RecordFormat::OggVorbis,
						RecordFormat::OggVorbis => RecordFormat::Flac,
					};
				}
			}
			RecordField::Directory => {
				// Enter and Esc are handled above, so the field can only report `Pending` here.
				let directories_only = |path: &Path| list_dir(path).into_iter().filter(|entry| entry.is_dir).collect();
				let _: PathOutcome = self.directory.handle_key(key, directories_only);
			}
		}
		RecordOutcome::Pending
	}
}

#[cfg(test)]
mod tests {
	use ratatui::crossterm::event::KeyModifiers;

	use super::*;

	fn press(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	fn fake_dir(_: &Path) -> Vec<DirEntry> {
		vec![
			DirEntry {
				name: "Music".into(),
				is_dir: true,
			},
			DirEntry {
				name: "Mu.flac".into(),
				is_dir: false,
			},
		]
	}

	#[test]
	fn enter_reports_the_folder_or_none_when_empty_and_esc_cancels() {
		let mut empty = RecordDialog::new(None, RecordFormat::Flac, 0.5);
		assert_eq!(
			empty.handle_key(press(KeyCode::Enter), fake_dir),
			RecordOutcome::Accepted {
				dir: None,
				format: RecordFormat::Flac
			}
		);
		let mut dialog = RecordDialog::new(Some(Path::new("D:/takes")), RecordFormat::OggVorbis, 0.5);
		assert_eq!(dialog.directory().text(), "D:/takes");
		assert_eq!(
			dialog.handle_key(press(KeyCode::Enter), fake_dir),
			RecordOutcome::Accepted {
				dir: Some(PathBuf::from("D:/takes")),
				format: RecordFormat::OggVorbis
			}
		);
		assert_eq!(dialog.handle_key(press(KeyCode::Esc), fake_dir), RecordOutcome::Cancelled);
	}

	#[test]
	fn arrows_move_the_focus_and_switch_the_format_only_on_the_format_row() {
		let mut dialog = RecordDialog::new(None, RecordFormat::Flac, 0.5);
		assert_eq!(dialog.focus(), RecordField::Directory);
		dialog.handle_key(press(KeyCode::Right), fake_dir);
		assert_eq!(dialog.format(), RecordFormat::Flac, "arrows edit the field, not the format");
		dialog.handle_key(press(KeyCode::Down), fake_dir);
		assert_eq!(dialog.focus(), RecordField::Format);
		dialog.handle_key(press(KeyCode::Right), fake_dir);
		assert_eq!(dialog.format(), RecordFormat::OggVorbis);
		dialog.handle_key(press(KeyCode::Left), fake_dir);
		assert_eq!(dialog.format(), RecordFormat::Flac);
		dialog.handle_key(press(KeyCode::Char(' ')), fake_dir);
		assert_eq!(dialog.format(), RecordFormat::OggVorbis);
		dialog.handle_key(press(KeyCode::Char('x')), fake_dir);
		assert_eq!(dialog.directory().text(), "", "typing on the format row does not reach the field");
		dialog.handle_key(press(KeyCode::Up), fake_dir);
		assert_eq!(dialog.focus(), RecordField::Directory);
	}

	#[test]
	fn the_field_types_pastes_and_completes_directories_only() {
		let mut dialog = RecordDialog::new(None, RecordFormat::Flac, 0.5);
		for c in "Mu".chars() {
			dialog.handle_key(press(KeyCode::Char(c)), fake_dir);
		}
		dialog.handle_key(press(KeyCode::Tab), fake_dir);
		assert_eq!(
			dialog.directory().text(),
			format!("Music{}", std::path::MAIN_SEPARATOR),
			"the file Mu.flac is not a candidate"
		);
		dialog.insert_str("takes");
		assert!(dialog.directory().text().ends_with("takes"));
		dialog.handle_key(press(KeyCode::Down), fake_dir);
		dialog.insert_str("!");
		assert!(dialog.directory().text().ends_with("takes!"), "paste always lands in the field");
	}
}
