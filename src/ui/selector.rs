//! Modal list selector (spec §6, dialog type 1): arrows / PgUp / PgDn / Home / End move, typed text filters,
//! Enter applies, Esc cancels. Pure state machine; drawing lives in `screen.rs`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorItem {
	pub label: String,
	/// Shown under the list while the item is highlighted (device rate and buffer range, format description).
	pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorOutcome {
	Pending,
	/// Index into the original item list.
	Chosen(usize),
	Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
	items: Vec<SelectorItem>,
	/// Original index of the item that is active now (marked in the list).
	current: Option<usize>,
	filter: String,
	/// Position within the filtered view.
	cursor: usize,
}

/// Rows moved by PgUp / PgDn.
pub const PAGE: usize = 10;

impl Selector {
	pub fn new(items: Vec<SelectorItem>, current: Option<usize>) -> Self {
		let cursor = current.unwrap_or(0);
		Self {
			items,
			current,
			filter: String::new(),
			cursor,
		}
	}

	pub fn filter(&self) -> &str {
		&self.filter
	}

	pub fn current(&self) -> Option<usize> {
		self.current
	}

	pub fn cursor(&self) -> usize {
		self.cursor
	}

	/// Original indices of the items matching the filter (case-insensitive substring), in list order.
	pub fn visible(&self) -> Vec<usize> {
		let needle = self.filter.to_lowercase();
		self.items
			.iter()
			.enumerate()
			.filter(|(_, item)| needle.is_empty() || item.label.to_lowercase().contains(&needle))
			.map(|(index, _)| index)
			.collect()
	}

	pub fn item(&self, index: usize) -> &SelectorItem {
		&self.items[index]
	}

	/// Original index of the highlighted item, if the filtered view is not empty.
	pub fn highlighted(&self) -> Option<usize> {
		self.visible().get(self.cursor).copied()
	}

	pub fn handle_key(&mut self, key: KeyEvent) -> SelectorOutcome {
		let visible_len = self.visible().len();
		let last = visible_len.saturating_sub(1);
		match key.code {
			KeyCode::Esc => return SelectorOutcome::Cancelled,
			KeyCode::Enter => return self.highlighted().map_or(SelectorOutcome::Pending, SelectorOutcome::Chosen),
			KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
			KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
			KeyCode::PageUp => self.cursor = self.cursor.saturating_sub(PAGE),
			KeyCode::PageDown => self.cursor = (self.cursor + PAGE).min(last),
			KeyCode::Home => self.cursor = 0,
			KeyCode::End => self.cursor = last,
			KeyCode::Backspace => {
				self.filter.pop();
				self.cursor = 0;
			}
			KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
				self.filter.push(c);
				self.cursor = 0;
			}
			_ => {}
		}
		SelectorOutcome::Pending
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn items(labels: &[&str]) -> Vec<SelectorItem> {
		labels
			.iter()
			.map(|label| SelectorItem {
				label: (*label).to_owned(),
				detail: None,
			})
			.collect()
	}

	fn selector() -> Selector {
		Selector::new(items(&["Arturia KeyStep", "loopMIDI Port", "Roland A-49", "Akai MPK"]), Some(1))
	}

	fn press(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	#[test]
	fn cursor_starts_on_the_current_item() {
		assert_eq!(selector().highlighted(), Some(1));
		assert_eq!(Selector::new(items(&["a"]), None).highlighted(), Some(0));
	}

	#[test]
	fn arrows_move_within_bounds() {
		let mut s = selector();
		s.handle_key(press(KeyCode::Down));
		s.handle_key(press(KeyCode::Down));
		s.handle_key(press(KeyCode::Down));
		s.handle_key(press(KeyCode::Down));
		assert_eq!(s.highlighted(), Some(3), "clamped at the end");
		s.handle_key(press(KeyCode::Home));
		assert_eq!(s.highlighted(), Some(0));
		s.handle_key(press(KeyCode::Up));
		assert_eq!(s.highlighted(), Some(0), "clamped at the start");
		s.handle_key(press(KeyCode::End));
		assert_eq!(s.highlighted(), Some(3));
		s.handle_key(press(KeyCode::PageUp));
		assert_eq!(s.highlighted(), Some(0));
		s.handle_key(press(KeyCode::PageDown));
		assert_eq!(s.highlighted(), Some(3));
	}

	#[test]
	fn typing_filters_case_insensitively_and_enter_returns_the_original_index() {
		let mut s = selector();
		for c in "AKAI".chars() {
			assert_eq!(s.handle_key(press(KeyCode::Char(c))), SelectorOutcome::Pending);
		}
		assert_eq!(s.visible(), vec![3]);
		assert_eq!(s.filter(), "AKAI");
		assert_eq!(s.handle_key(press(KeyCode::Enter)), SelectorOutcome::Chosen(3));
	}

	#[test]
	fn backspace_widens_the_filter_again() {
		let mut s = selector();
		s.handle_key(press(KeyCode::Char('o')));
		assert_eq!(s.visible(), vec![1, 2]);
		s.handle_key(press(KeyCode::Backspace));
		assert_eq!(s.visible().len(), 4);
	}

	#[test]
	fn enter_on_an_empty_result_does_nothing_and_esc_cancels() {
		let mut s = selector();
		s.handle_key(press(KeyCode::Char('z')));
		assert!(s.visible().is_empty());
		assert_eq!(s.handle_key(press(KeyCode::Enter)), SelectorOutcome::Pending);
		assert_eq!(s.handle_key(press(KeyCode::Esc)), SelectorOutcome::Cancelled);
	}

	#[test]
	fn control_chords_do_not_type_into_the_filter() {
		let mut s = selector();
		s.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
		assert_eq!(s.filter(), "");
	}
}
