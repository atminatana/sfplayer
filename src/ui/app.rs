//! Application state and the event loop (≤ 30 frames/s, never blocks the audio thread).
//!
//! `App` never touches devices: key handling yields a `Request`, which `fulfil` applies to the `Session`.
//! That keeps dialogs and hotkeys testable without hardware.

use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};

use super::dialog::{Choice, Dialog, DialogKind};
use super::keys::{Action, action_for};
use super::screen;
use super::selector::SelectorOutcome;
use crate::audio;
use crate::session::{Notice, Session};

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

/// What the UI wants the session to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
	OpenDialog(DialogKind),
	Apply(Choice),
	Rescan,
}

pub struct App {
	pub locale: &'static str,
	pub focus: Panel,
	pub should_quit: bool,
	pub dialog: Option<Dialog>,
	dirty: bool,
}

impl App {
	pub fn new(locale: &'static str) -> Self {
		Self {
			locale,
			focus: Panel::Monitor,
			should_quit: false,
			dialog: None,
			dirty: true,
		}
	}

	/// Handles one key press. Quit works everywhere; with a dialog open every other key belongs to the dialog.
	pub fn handle_key(&mut self, key: KeyEvent) -> Option<Request> {
		if action_for(&key) == Some(Action::Quit) {
			self.should_quit = true;
			self.dirty = true;
			return None;
		}
		if let Some(dialog) = self.dialog.as_mut() {
			self.dirty = true;
			return match dialog.selector.handle_key(key) {
				SelectorOutcome::Pending => None,
				SelectorOutcome::Cancelled => {
					self.dialog = None;
					None
				}
				SelectorOutcome::Chosen(row) => {
					let choice = dialog.choice(row);
					self.dialog = None;
					choice.map(Request::Apply)
				}
			};
		}
		let action = action_for(&key)?;
		self.dirty = true;
		match action {
			Action::Quit => unreachable!("handled above"),
			Action::Help => None,
			Action::FocusNext | Action::FocusPrevious => {
				self.focus = self.focus.next();
				None
			}
			Action::MidiDialog => Some(Request::OpenDialog(DialogKind::MidiInput)),
			Action::AudioDialog => Some(Request::OpenDialog(DialogKind::AudioOutput)),
			Action::PresetDialog => Some(Request::OpenDialog(DialogKind::Preset)),
			Action::RecordFormatDialog => Some(Request::OpenDialog(DialogKind::RecordFormat)),
			Action::Rescan => Some(Request::Rescan),
		}
	}

	pub fn open_dialog(&mut self, dialog: Dialog) {
		self.dialog = Some(dialog);
		self.dirty = true;
	}

	/// Marks the screen for redraw without changing state (terminal resize).
	pub fn mark_dirty(&mut self) {
		self.dirty = true;
	}

	/// Returns and clears the redraw flag.
	pub fn take_dirty(&mut self) -> bool {
		std::mem::take(&mut self.dirty)
	}
}

/// Applies a UI request to the session and the app (dialog contents come from the session).
pub fn fulfil(request: Request, app: &mut App, session: &mut Session) {
	match request {
		Request::OpenDialog(DialogKind::MidiInput) => {
			session.rescan();
			app.open_dialog(Dialog::midi(&session.midi_ports, session.connected_midi_port()));
		}
		Request::OpenDialog(DialogKind::AudioOutput) => match audio::list_output_devices() {
			Ok(devices) => {
				let current = session.audio().map(|stream| stream.device_name.clone());
				app.open_dialog(Dialog::audio(&devices, current.as_deref(), app.locale));
			}
			Err(error) => session.notice = Some(Notice::AudioError(error.to_string())),
		},
		Request::OpenDialog(DialogKind::Preset) => {
			if let Some(loaded) = &session.bank {
				let channel = session.current_channel;
				app.open_dialog(Dialog::preset(loaded.bank.presets(), session.preset_for(channel)));
			}
		}
		Request::OpenDialog(DialogKind::RecordFormat) => {
			app.open_dialog(Dialog::record_format(
				session.config.record.format,
				session.config.record.vorbis_quality,
				app.locale,
			));
		}
		Request::Apply(Choice::ConnectMidi(index)) => session.connect_midi(index),
		Request::Apply(Choice::OpenAudio(index)) => session.reopen_audio(index),
		Request::Apply(Choice::SelectPreset { bank, program }) => {
			let channel = session.current_channel;
			session.select_preset(channel, bank, program);
		}
		Request::Apply(Choice::SetRecordFormat(format)) => session.set_record_format(format),
		Request::Rescan => session.rescan(),
	}
}

/// Runs the UI until the user quits. Input is polled with a timeout so the loop also wakes up to refresh meters.
pub fn run(terminal: &mut DefaultTerminal, app: &mut App, session: &mut Session) -> std::io::Result<()> {
	let mut next_frame = Instant::now();
	loop {
		session.poll();
		let now = Instant::now();
		if app.take_dirty() || now >= next_frame {
			terminal.draw(|frame| screen::render(frame, app, &screen::View::from_session(session)))?;
			next_frame = now + FRAME_INTERVAL;
		}
		let timeout = next_frame.saturating_duration_since(Instant::now());
		if event::poll(timeout)? {
			match event::read()? {
				Event::Key(key) if key.kind != KeyEventKind::Release => {
					if let Some(request) = app.handle_key(key) {
						fulfil(request, app, session);
					}
				}
				Event::Resize(_, _) => app.mark_dirty(),
				_ => {}
			}
		}
		if app.should_quit {
			return Ok(());
		}
	}
}

#[cfg(test)]
mod tests {
	use ratatui::crossterm::event::{KeyCode, KeyModifiers};

	use super::*;
	use crate::midi::MidiPortInfo;

	fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
		KeyEvent::new(code, modifiers)
	}

	fn plain(code: KeyCode) -> KeyEvent {
		press(code, KeyModifiers::NONE)
	}

	fn midi_dialog() -> Dialog {
		Dialog::midi(
			&[
				MidiPortInfo {
					index: 0,
					name: "KeyStep".into(),
				},
				MidiPortInfo {
					index: 1,
					name: "loopMIDI".into(),
				},
			],
			None,
		)
	}

	#[test]
	fn tab_cycles_focus_between_the_two_panels() {
		let mut app = App::new("en");
		assert_eq!(app.focus, Panel::Monitor);
		assert_eq!(app.handle_key(plain(KeyCode::Tab)), None);
		assert_eq!(app.focus, Panel::Mixer);
		assert_eq!(app.handle_key(press(KeyCode::BackTab, KeyModifiers::SHIFT)), None);
		assert_eq!(app.focus, Panel::Monitor);
		assert!(app.take_dirty());
	}

	#[test]
	fn quit_keys_set_the_flag_even_inside_a_dialog() {
		let mut app = App::new("en");
		app.handle_key(plain(KeyCode::F(10)));
		assert!(app.should_quit);
		let mut app = App::new("en");
		app.open_dialog(midi_dialog());
		app.handle_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL));
		assert!(app.should_quit);
	}

	#[test]
	fn unbound_keys_do_nothing_and_need_no_redraw() {
		let mut app = App::new("en");
		app.take_dirty();
		assert_eq!(app.handle_key(plain(KeyCode::Char('z'))), None);
		assert!(!app.take_dirty());
		assert!(!app.should_quit);
	}

	#[test]
	fn function_keys_request_dialogs_and_rescan() {
		let mut app = App::new("en");
		assert_eq!(
			app.handle_key(plain(KeyCode::F(2))),
			Some(Request::OpenDialog(DialogKind::MidiInput))
		);
		assert_eq!(
			app.handle_key(plain(KeyCode::F(3))),
			Some(Request::OpenDialog(DialogKind::AudioOutput))
		);
		assert_eq!(app.handle_key(plain(KeyCode::F(5))), Some(Request::OpenDialog(DialogKind::Preset)));
		assert_eq!(
			app.handle_key(plain(KeyCode::F(6))),
			Some(Request::OpenDialog(DialogKind::RecordFormat))
		);
		assert_eq!(app.handle_key(plain(KeyCode::F(8))), Some(Request::Rescan));
	}

	#[test]
	fn dialog_captures_keys_until_enter_or_esc() {
		let mut app = App::new("en");
		app.open_dialog(midi_dialog());
		assert_eq!(app.handle_key(plain(KeyCode::Tab)), None, "Tab goes to the dialog, not to focus");
		assert_eq!(app.focus, Panel::Monitor);
		assert_eq!(
			app.handle_key(plain(KeyCode::F(3))),
			None,
			"other hotkeys are swallowed while a dialog is open"
		);
		assert!(app.dialog.is_some());
		for c in "loop".chars() {
			app.handle_key(plain(KeyCode::Char(c)));
		}
		assert_eq!(app.dialog.as_ref().unwrap().selector.filter(), "loop");
		assert_eq!(app.handle_key(plain(KeyCode::Enter)), Some(Request::Apply(Choice::ConnectMidi(1))));
		assert!(app.dialog.is_none());

		app.open_dialog(midi_dialog());
		assert_eq!(app.handle_key(plain(KeyCode::Esc)), None);
		assert!(app.dialog.is_none());
	}
}
