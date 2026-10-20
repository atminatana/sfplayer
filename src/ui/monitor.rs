//! MIDI monitor view state (spec F-08): the last N messages, scrolling, pause and clear.

use std::collections::VecDeque;

use crate::midi::MonitorEntry;

/// Spec F-08 default history length.
pub const DEFAULT_CAPACITY: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
	entries: VecDeque<MonitorEntry>,
	capacity: usize,
	/// Rows scrolled up from the newest entry; 0 = follow the tail.
	scroll: usize,
	paused: bool,
	/// Entries received while paused are kept but not shown until resume.
	hidden_while_paused: usize,
	total: u64,
}

impl Monitor {
	pub fn new(capacity: usize) -> Self {
		Self {
			entries: VecDeque::with_capacity(capacity),
			capacity,
			scroll: 0,
			paused: false,
			hidden_while_paused: 0,
			total: 0,
		}
	}

	pub fn push(&mut self, entry: MonitorEntry) {
		if self.entries.len() == self.capacity {
			self.entries.pop_front();
			if self.paused {
				self.hidden_while_paused = self.hidden_while_paused.saturating_sub(1);
			}
		}
		self.entries.push_back(entry);
		self.total += 1;
		if self.paused {
			self.hidden_while_paused = (self.hidden_while_paused + 1).min(self.entries.len());
		}
	}

	pub fn extend(&mut self, entries: impl IntoIterator<Item = MonitorEntry>) {
		for entry in entries {
			self.push(entry);
		}
	}

	pub fn clear(&mut self) {
		self.entries.clear();
		self.scroll = 0;
		self.hidden_while_paused = 0;
	}

	pub fn toggle_pause(&mut self) {
		self.paused = !self.paused;
		if !self.paused {
			self.hidden_while_paused = 0;
		}
	}

	pub fn is_paused(&self) -> bool {
		self.paused
	}

	pub fn len(&self) -> usize {
		self.entries.len()
	}

	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}

	pub fn total(&self) -> u64 {
		self.total
	}

	pub fn scroll_up(&mut self, rows: usize) {
		self.scroll = (self.scroll + rows).min(self.visible_len().saturating_sub(1));
	}

	pub fn scroll_down(&mut self, rows: usize) {
		self.scroll = self.scroll.saturating_sub(rows);
	}

	pub fn scroll_to_top(&mut self) {
		self.scroll = self.visible_len().saturating_sub(1);
	}

	pub fn scroll_to_bottom(&mut self) {
		self.scroll = 0;
	}

	pub fn scroll(&self) -> usize {
		self.scroll
	}

	/// Entries the view may show: everything, minus what arrived during a pause.
	fn visible_len(&self) -> usize {
		self.entries.len() - self.hidden_while_paused
	}

	/// The `rows` entries ending `scroll` rows above the newest visible one, oldest first.
	pub fn window(&self, rows: usize) -> impl Iterator<Item = &MonitorEntry> {
		let visible = self.visible_len();
		let end = visible.saturating_sub(self.scroll);
		let start = end.saturating_sub(rows);
		self.entries.range(start..end)
	}
}

impl Default for Monitor {
	fn default() -> Self {
		Self::new(DEFAULT_CAPACITY)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::midi::MidiEvent;

	fn entry(key: u8) -> MonitorEntry {
		MonitorEntry::now(MidiEvent::NoteOn {
			channel: 0,
			key,
			velocity: 100,
		})
	}

	fn keys(monitor: &Monitor, rows: usize) -> Vec<u8> {
		monitor
			.window(rows)
			.map(|entry| match entry.event {
				MidiEvent::NoteOn { key, .. } => key,
				_ => unreachable!(),
			})
			.collect()
	}

	#[test]
	fn keeps_only_the_last_capacity_entries() {
		let mut monitor = Monitor::new(3);
		monitor.extend((1..=5).map(entry));
		assert_eq!(monitor.len(), 3);
		assert_eq!(monitor.total(), 5);
		assert_eq!(keys(&monitor, 10), vec![3, 4, 5]);
	}

	#[test]
	fn window_follows_the_tail_and_scrolls_within_bounds() {
		let mut monitor = Monitor::new(10);
		monitor.extend((1..=6).map(entry));
		assert_eq!(keys(&monitor, 3), vec![4, 5, 6]);
		monitor.scroll_up(2);
		assert_eq!(keys(&monitor, 3), vec![2, 3, 4]);
		monitor.scroll_up(100);
		assert_eq!(monitor.scroll(), 5, "clamped so the oldest entry stays visible");
		assert_eq!(keys(&monitor, 3), vec![1]);
		monitor.scroll_to_bottom();
		assert_eq!(keys(&monitor, 3), vec![4, 5, 6]);
		monitor.scroll_to_top();
		assert_eq!(keys(&monitor, 1), vec![1]);
		monitor.scroll_down(100);
		assert_eq!(monitor.scroll(), 0);
	}

	#[test]
	fn pause_freezes_the_view_but_keeps_collecting() {
		let mut monitor = Monitor::new(10);
		monitor.extend((1..=2).map(entry));
		monitor.toggle_pause();
		monitor.extend((3..=4).map(entry));
		assert!(monitor.is_paused());
		assert_eq!(keys(&monitor, 10), vec![1, 2], "new entries hidden while paused");
		assert_eq!(monitor.len(), 4);
		monitor.toggle_pause();
		assert_eq!(keys(&monitor, 10), vec![1, 2, 3, 4]);
	}

	#[test]
	fn clear_empties_and_resets_scroll() {
		let mut monitor = Monitor::new(10);
		monitor.extend((1..=3).map(entry));
		monitor.scroll_up(1);
		monitor.clear();
		assert!(monitor.is_empty());
		assert_eq!(monitor.scroll(), 0);
		assert_eq!(keys(&monitor, 5), Vec::<u8>::new());
	}
}
