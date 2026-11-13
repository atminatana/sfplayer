//! Asking the user for a path (spec v1.2 §4.2): what is asked, and reading a directory for the
//! suggestions of the input dialog. No drawing here; the dialog itself is `input_dialog`.

use std::path::{Path, PathBuf};

/// What the input dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
	/// A SoundFont bank to load.
	Sf2File,
	/// The folder takes are written to.
	RecordDir,
}

/// A request to show the input dialog: what to ask, what the field starts with, and the paths entered
/// before, most recent first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRequest {
	pub kind: InputKind,
	pub initial: String,
	pub history: Vec<PathBuf>,
}

/// One directory entry as the suggestions see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
	pub name: String,
	pub is_dir: bool,
}

/// Splits "dir/part" into ("dir/", "part"); both separators count on every platform.
pub(super) fn split_dir_and_prefix(text: &str) -> (String, String) {
	match text.rfind(['/', '\\']) {
		Some(index) => (text[..=index].to_owned(), text[index + 1..].to_owned()),
		None => (String::new(), text.to_owned()),
	}
}

/// Real directory listing, sorted by name without regard to case; an unreadable directory lists nothing.
pub fn list_dir(path: &Path) -> Vec<DirEntry> {
	let Ok(entries) = std::fs::read_dir(path) else {
		return Vec::new();
	};
	let mut listed: Vec<DirEntry> = entries
		.filter_map(Result::ok)
		.map(|entry| DirEntry {
			name: entry.file_name().to_string_lossy().into_owned(),
			is_dir: entry.path().is_dir(),
		})
		.collect();
	listed.sort_by_key(|entry| entry.name.to_lowercase());
	listed
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_path_splits_into_its_directory_and_the_name_being_typed() {
		assert_eq!(split_dir_and_prefix("D:/sf2/Gen"), ("D:/sf2/".to_owned(), "Gen".to_owned()));
		assert_eq!(split_dir_and_prefix("D:\\sf2\\"), ("D:\\sf2\\".to_owned(), String::new()));
		assert_eq!(split_dir_and_prefix("bank"), (String::new(), "bank".to_owned()));
	}

	#[test]
	fn real_listing_reads_the_repository_assets_directory() {
		let entries = list_dir(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")));
		assert!(entries.iter().any(|entry| entry.name == "sin_wave.sf2" && !entry.is_dir));
		assert!(list_dir(Path::new("definitely/not/here")).is_empty());
	}
}
