//! Text field for a file path (spec §6, dialog type 2 / F-05): cursor editing, Tab completion,
//! a list of recent banks below the field. Pure state; the directory listing is injected so it is testable.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathOutcome {
	Pending,
	Submit(PathBuf),
	Cancelled,
}

/// One directory entry as seen by the completer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
	pub name: String,
	pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathInput {
	text: Vec<char>,
	cursor: usize,
	recent: Vec<PathBuf>,
	recent_cursor: Option<usize>,
	error: Option<String>,
}

impl PathInput {
	pub fn new(initial: &str, recent: Vec<PathBuf>) -> Self {
		let text: Vec<char> = initial.chars().collect();
		let cursor = text.len();
		Self {
			text,
			cursor,
			recent,
			recent_cursor: None,
			error: None,
		}
	}

	pub fn text(&self) -> String {
		self.text.iter().collect()
	}

	/// Cursor position in characters.
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	pub fn recent(&self) -> &[PathBuf] {
		&self.recent
	}

	pub fn recent_cursor(&self) -> Option<usize> {
		self.recent_cursor
	}

	pub fn error(&self) -> Option<&str> {
		self.error.as_deref()
	}

	/// Shown in red under the field; the dialog stays open (spec §6).
	pub fn set_error(&mut self, message: String) {
		self.error = Some(message);
	}

	pub fn insert_str(&mut self, pasted: &str) {
		for c in pasted.chars().filter(|c| !c.is_control()) {
			self.text.insert(self.cursor, c);
			self.cursor += 1;
		}
		self.edited();
	}

	fn edited(&mut self) {
		self.recent_cursor = None;
		self.error = None;
	}

	/// Handles a key; Tab completion uses `list_dir` to read the directory being typed.
	pub fn handle_key(&mut self, key: KeyEvent, list_dir: impl Fn(&Path) -> Vec<DirEntry>) -> PathOutcome {
		match key.code {
			KeyCode::Esc => return PathOutcome::Cancelled,
			KeyCode::Enter => {
				let text = self.text().trim().to_owned();
				if !text.is_empty() {
					return PathOutcome::Submit(PathBuf::from(text));
				}
			}
			KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
			KeyCode::Right => self.cursor = (self.cursor + 1).min(self.text.len()),
			KeyCode::Home => self.cursor = 0,
			KeyCode::End => self.cursor = self.text.len(),
			KeyCode::Backspace => {
				if self.cursor > 0 {
					self.cursor -= 1;
					self.text.remove(self.cursor);
					self.edited();
				}
			}
			KeyCode::Delete => {
				if self.cursor < self.text.len() {
					self.text.remove(self.cursor);
					self.edited();
				}
			}
			KeyCode::Tab => self.complete(list_dir),
			KeyCode::Up => self.move_recent(-1),
			KeyCode::Down => self.move_recent(1),
			KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
				self.text.insert(self.cursor, c);
				self.cursor += 1;
				self.edited();
			}
			_ => {}
		}
		PathOutcome::Pending
	}

	/// ↑/↓ walk the recent list and copy the highlighted path into the field.
	fn move_recent(&mut self, delta: isize) {
		if self.recent.is_empty() {
			return;
		}
		let last = self.recent.len() - 1;
		let next = match (self.recent_cursor, delta) {
			(None, d) if d < 0 => last,
			(None, _) => 0,
			(Some(i), d) if d < 0 => i.saturating_sub(1),
			(Some(i), _) => (i + 1).min(last),
		};
		self.recent_cursor = Some(next);
		self.text = self.recent[next].to_string_lossy().chars().collect();
		self.cursor = self.text.len();
		self.error = None;
	}

	/// Completes the last path component: one match → full name (plus separator for a directory),
	/// several → their longest common prefix, none → unchanged.
	fn complete(&mut self, list_dir: impl Fn(&Path) -> Vec<DirEntry>) {
		let text = self.text();
		let (dir, prefix) = split_dir_and_prefix(&text);
		let candidates: Vec<DirEntry> = list_dir(Path::new(if dir.is_empty() { "." } else { &dir }))
			.into_iter()
			.filter(|entry| entry.name.to_lowercase().starts_with(&prefix.to_lowercase()))
			.collect();
		let completed = match candidates.as_slice() {
			[] => return,
			[single] => {
				let mut name = single.name.clone();
				if single.is_dir {
					name.push(std::path::MAIN_SEPARATOR);
				}
				name
			}
			many => common_prefix(many.iter().map(|entry| entry.name.as_str())),
		};
		if completed.chars().count() <= prefix.chars().count() {
			return;
		}
		self.text = format!("{dir}{completed}").chars().collect();
		self.cursor = self.text.len();
		self.edited();
	}
}

/// Splits "dir/part" into ("dir/", "part"); both separators count on every platform.
fn split_dir_and_prefix(text: &str) -> (String, String) {
	match text.rfind(['/', '\\']) {
		Some(index) => (text[..=index].to_owned(), text[index + 1..].to_owned()),
		None => (String::new(), text.to_owned()),
	}
}

fn common_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> String {
	let Some(first) = names.next() else {
		return String::new();
	};
	let mut prefix: Vec<char> = first.chars().collect();
	for name in names {
		let keep = prefix
			.iter()
			.zip(name.chars())
			.take_while(|(a, b)| a.eq_ignore_ascii_case(b))
			.count();
		prefix.truncate(keep);
	}
	prefix.into_iter().collect()
}

/// Real directory listing for Tab completion.
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

	fn press(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	fn fake_dir(_: &Path) -> Vec<DirEntry> {
		vec![
			DirEntry {
				name: "banks".into(),
				is_dir: true,
			},
			DirEntry {
				name: "GeneralUser.sf2".into(),
				is_dir: false,
			},
			DirEntry {
				name: "Generic.sf2".into(),
				is_dir: false,
			},
			DirEntry {
				name: "sin_wave.sf2".into(),
				is_dir: false,
			},
		]
	}

	fn type_text(input: &mut PathInput, text: &str) {
		for c in text.chars() {
			input.handle_key(press(KeyCode::Char(c)), fake_dir);
		}
	}

	#[test]
	fn typing_and_cursor_movement_edit_the_text() {
		let mut input = PathInput::new("", vec![]);
		type_text(&mut input, "abd");
		input.handle_key(press(KeyCode::Left), fake_dir);
		type_text(&mut input, "c");
		assert_eq!(input.text(), "abcd");
		assert_eq!(input.cursor(), 3);
		input.handle_key(press(KeyCode::Home), fake_dir);
		input.handle_key(press(KeyCode::Delete), fake_dir);
		assert_eq!(input.text(), "bcd");
		input.handle_key(press(KeyCode::End), fake_dir);
		input.handle_key(press(KeyCode::Backspace), fake_dir);
		assert_eq!(input.text(), "bc");
	}

	#[test]
	fn enter_submits_trimmed_non_empty_text_and_esc_cancels() {
		let mut input = PathInput::new("  bank.sf2 ", vec![]);
		assert_eq!(
			input.handle_key(press(KeyCode::Enter), fake_dir),
			PathOutcome::Submit(PathBuf::from("bank.sf2"))
		);
		let mut empty = PathInput::new("   ", vec![]);
		assert_eq!(empty.handle_key(press(KeyCode::Enter), fake_dir), PathOutcome::Pending);
		assert_eq!(empty.handle_key(press(KeyCode::Esc), fake_dir), PathOutcome::Cancelled);
	}

	#[test]
	fn tab_completes_a_unique_match_and_marks_directories() {
		let mut input = PathInput::new("assets/sin", vec![]);
		input.handle_key(press(KeyCode::Tab), fake_dir);
		assert_eq!(input.text(), "assets/sin_wave.sf2");
		let mut dir = PathInput::new("ba", vec![]);
		dir.handle_key(press(KeyCode::Tab), fake_dir);
		assert_eq!(dir.text(), format!("banks{}", std::path::MAIN_SEPARATOR));
	}

	#[test]
	fn tab_with_several_matches_completes_the_common_prefix_only() {
		let mut input = PathInput::new("gen", vec![]);
		input.handle_key(press(KeyCode::Tab), fake_dir);
		assert_eq!(input.text(), "Gener");
		let mut none = PathInput::new("zzz", vec![]);
		none.handle_key(press(KeyCode::Tab), fake_dir);
		assert_eq!(none.text(), "zzz");
	}

	#[test]
	fn arrows_walk_the_recent_list_into_the_field() {
		let recent = vec![PathBuf::from("a.sf2"), PathBuf::from("b.sf2"), PathBuf::from("c.sf2")];
		let mut input = PathInput::new("", recent);
		input.handle_key(press(KeyCode::Down), fake_dir);
		assert_eq!((input.text(), input.recent_cursor()), ("a.sf2".to_owned(), Some(0)));
		input.handle_key(press(KeyCode::Down), fake_dir);
		assert_eq!(input.text(), "b.sf2");
		input.handle_key(press(KeyCode::Up), fake_dir);
		assert_eq!(input.text(), "a.sf2");
		type_text(&mut input, "x");
		assert_eq!(input.recent_cursor(), None, "typing leaves the recent list");
		let mut from_end = PathInput::new("", vec![PathBuf::from("only.sf2")]);
		from_end.handle_key(press(KeyCode::Up), fake_dir);
		assert_eq!(from_end.text(), "only.sf2");
	}

	#[test]
	fn error_is_cleared_by_editing_and_paste_skips_control_characters() {
		let mut input = PathInput::new("", vec![]);
		input.set_error("not a SoundFont".into());
		assert_eq!(input.error(), Some("not a SoundFont"));
		input.insert_str("C:\\ba\nnk.sf2");
		assert_eq!(input.text(), "C:\\bank.sf2");
		assert_eq!(input.error(), None);
	}

	#[test]
	fn real_listing_reads_the_repository_assets_directory() {
		let entries = list_dir(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")));
		assert!(entries.iter().any(|entry| entry.name == "sin_wave.sf2" && !entry.is_dir));
		assert!(list_dir(Path::new("definitely/not/here")).is_empty());
	}
}
