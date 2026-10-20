//! Application state and the event loop (≤ 30 frames/s, never blocks the audio thread).

use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};

use super::keys::{Action, action_for};
use super::screen;
use crate::session::Session;

/// Spec §6: redraw at most 30 frames per second.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
	Monitor,
	Mixer,
}

impl Panel {
	fn next(self) -> Self {
		match self {
			Self::Monitor => Self::Mixer,
			Self::Mixer => Self::Monitor,
		}
	}
}

pub struct App {
	pub locale: &'static str,
	pub focus: Panel,
	pub should_quit: bool,
}

impl App {
	pub fn new(locale: &'static str) -> Self {
		Self {
			locale,
			focus: Panel::Monitor,
			should_quit: false,
		}
	}

	/// Handles one key press. Returns `true` when the screen needs a redraw before the next frame tick.
	pub fn handle_key(&mut self, key: KeyEvent) -> bool {
		match action_for(&key) {
			Some(Action::Quit) => self.should_quit = true,
			Some(Action::FocusNext) | Some(Action::FocusPrevious) => self.focus = self.focus.next(),
			Some(Action::Help) | None => return false,
		}
		true
	}
}

/// Runs the UI until the user quits. Input is polled with a timeout so the loop also wakes up to refresh meters.
pub fn run(terminal: &mut DefaultTerminal, app: &mut App, session: &mut Session) -> std::io::Result<()> {
	let mut next_frame = Instant::now();
	let mut dirty = true;
	loop {
		session.poll();
		let now = Instant::now();
		if dirty || now >= next_frame {
			terminal.draw(|frame| screen::render(frame, app, &screen::View::from_session(session)))?;
			next_frame = now + FRAME_INTERVAL;
			dirty = false;
		}
		let timeout = next_frame.saturating_duration_since(Instant::now());
		if event::poll(timeout)? {
			match event::read()? {
				Event::Key(key) if key.kind != KeyEventKind::Release => dirty |= app.handle_key(key),
				Event::Resize(_, _) => dirty = true,
				_ => {}
			}
		}
		if app.should_quit {
			return Ok(());
		}
	}
}
