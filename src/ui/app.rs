//! Application state and the event loop (≤ 30 frames/s, never blocks the audio thread).
//!
//! `App` never touches devices: key handling yields a `Request`, which `fulfil` applies to the `Session`.
//! That keeps dialogs and hotkeys testable without hardware.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};

use super::dialog::{Choice, Dialog, DialogKind};
use super::keys::{Action, action_for};
use super::meter::ClipHold;
use super::monitor::Monitor;
use super::path_input::{self, PathInput, PathOutcome};
use super::screen;
use super::selector::SelectorOutcome;
use crate::audio;
use crate::session::{Notice, Session};

/// Spec §6: redraw at most 30 frames per second.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// Rows moved by PgUp / PgDn in the monitor.
const MONITOR_PAGE: usize = 10;

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
	OpenSf2Dialog,
	Apply(Choice),
	LoadBank(PathBuf),
	Rescan,
}

/// The modal window on top of the main screen, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
	Select(Dialog),
	Sf2Path(PathInput),
	/// F1: scroll offset in rows.
	Help(usize),
}

pub struct App {
	pub locale: &'static str,
	pub focus: Panel,
	pub should_quit: bool,
	pub modal: Option<Modal>,
	pub monitor: Monitor,
	clip_hold: ClipHold,
	/// Whether the CLIP mark is lit for the frame being drawn.
	pub clipping: bool,
	dirty: bool,
}

impl App {
	pub fn new(locale: &'static str) -> Self {
		Self {
			locale,
			focus: Panel::Monitor,
			should_quit: false,
			modal: None,
			monitor: Monitor::default(),
			clip_hold: ClipHold::new(0),
			clipping: false,
			dirty: true,
		}
	}

	/// Per-frame meter bookkeeping: feeds the clip counter and resolves the CLIP mark for `now`.
	pub fn update_meters(&mut self, clips: u32, now: Instant) {
		self.clip_hold.update(clips, now);
		self.clipping = self.clip_hold.is_lit(now);
	}

	/// Handles one key press. Quit works everywhere; with a modal open every other key belongs to it.
	pub fn handle_key(&mut self, key: KeyEvent) -> Option<Request> {
		if action_for(&key) == Some(Action::Quit) {
			self.should_quit = true;
			self.dirty = true;
			return None;
		}
		if self.modal.is_some() {
			self.dirty = true;
			return self.handle_modal_key(key);
		}
		let action = action_for(&key)?;
		self.dirty = true;
		match action {
			Action::Quit => unreachable!("handled above"),
			Action::Help => {
				self.modal = Some(Modal::Help(0));
				None
			}
			Action::FocusNext | Action::FocusPrevious => {
				self.focus = self.focus.next();
				None
			}
			Action::MidiDialog => Some(Request::OpenDialog(DialogKind::MidiInput)),
			Action::AudioDialog => Some(Request::OpenDialog(DialogKind::AudioOutput)),
			Action::Sf2Dialog => Some(Request::OpenSf2Dialog),
			Action::PresetDialog => Some(Request::OpenDialog(DialogKind::Preset)),
			Action::RecordFormatDialog => Some(Request::OpenDialog(DialogKind::RecordFormat)),
			Action::Rescan => Some(Request::Rescan),
			Action::MonitorClear
			| Action::MonitorPause
			| Action::ScrollUp
			| Action::ScrollDown
			| Action::PageUp
			| Action::PageDown
			| Action::ScrollHome
			| Action::ScrollEnd => {
				self.handle_panel_action(action);
				None
			}
		}
	}

	/// Keys whose meaning depends on the focused panel (spec §6 "контекст").
	fn handle_panel_action(&mut self, action: Action) {
		if self.focus != Panel::Monitor {
			return;
		}
		match action {
			Action::MonitorClear => self.monitor.clear(),
			Action::MonitorPause => self.monitor.toggle_pause(),
			Action::ScrollUp => self.monitor.scroll_up(1),
			Action::ScrollDown => self.monitor.scroll_down(1),
			Action::PageUp => self.monitor.scroll_up(MONITOR_PAGE),
			Action::PageDown => self.monitor.scroll_down(MONITOR_PAGE),
			Action::ScrollHome => self.monitor.scroll_to_top(),
			Action::ScrollEnd => self.monitor.scroll_to_bottom(),
			_ => {}
		}
	}

	fn handle_modal_key(&mut self, key: KeyEvent) -> Option<Request> {
		match self.modal.as_mut()? {
			Modal::Select(dialog) => match dialog.selector.handle_key(key) {
				SelectorOutcome::Pending => None,
				SelectorOutcome::Cancelled => {
					self.modal = None;
					None
				}
				SelectorOutcome::Chosen(row) => {
					let choice = dialog.choice(row);
					self.modal = None;
					choice.map(Request::Apply)
				}
			},
			Modal::Help(scroll) => {
				match key.code {
					KeyCode::Esc | KeyCode::F(1) | KeyCode::Enter => self.modal = None,
					KeyCode::Up => *scroll = scroll.saturating_sub(1),
					KeyCode::Down => *scroll += 1,
					KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
					KeyCode::PageDown => *scroll += 10,
					KeyCode::Home => *scroll = 0,
					_ => {}
				}
				None
			}
			Modal::Sf2Path(input) => match input.handle_key(key, path_input::list_dir) {
				PathOutcome::Pending => None,
				PathOutcome::Cancelled => {
					self.modal = None;
					None
				}
				// The dialog stays open: `fulfil` closes it on success or shows the error inside it.
				PathOutcome::Submit(path) => Some(Request::LoadBank(path)),
			},
		}
	}

	/// Terminal paste (bracketed paste) goes into the path field; elsewhere it is ignored.
	pub fn handle_paste(&mut self, text: &str) {
		if let Some(Modal::Sf2Path(input)) = self.modal.as_mut() {
			input.insert_str(text);
			self.dirty = true;
		}
	}

	pub fn open_modal(&mut self, modal: Modal) {
		self.modal = Some(modal);
		self.dirty = true;
	}

	pub fn open_dialog(&mut self, dialog: Dialog) {
		self.open_modal(Modal::Select(dialog));
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
		Request::OpenSf2Dialog => {
			let initial = session
				.bank
				.as_ref()
				.map(|loaded| loaded.path.to_string_lossy().into_owned())
				.unwrap_or_default();
			app.open_modal(Modal::Sf2Path(PathInput::new(&initial, session.config.synth.recent_banks.clone())));
		}
		Request::Apply(Choice::ConnectMidi(index)) => session.connect_midi(index),
		Request::Apply(Choice::OpenAudio(index)) => session.reopen_audio(index),
		Request::Apply(Choice::SelectPreset { bank, program }) => {
			let channel = session.current_channel;
			session.select_preset(channel, bank, program);
		}
		Request::Apply(Choice::SetRecordFormat(format)) => session.set_record_format(format),
		Request::LoadBank(path) => match session.load_bank(&path) {
			Ok(()) => app.modal = None,
			Err(error) => {
				if let Some(Modal::Sf2Path(input)) = app.modal.as_mut() {
					input.set_error(error.to_string());
				}
			}
		},
		Request::Rescan => session.rescan(),
	}
}

/// Runs the UI until the user quits. Input is polled with a timeout so the loop also wakes up to refresh meters.
pub fn run(terminal: &mut DefaultTerminal, app: &mut App, session: &mut Session) -> std::io::Result<()> {
	let mut next_frame = Instant::now();
	loop {
		session.poll();
		let before = app.monitor.total();
		app.monitor.extend(session.drain_monitor());
		if app.monitor.total() != before {
			app.mark_dirty();
		}
		let now = Instant::now();
		app.update_meters(session.shared.clips(), now);
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
				Event::Paste(text) => app.handle_paste(&text),
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
		assert_eq!(app.handle_key(plain(KeyCode::F(4))), Some(Request::OpenSf2Dialog));
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
		assert!(app.modal.is_some());
		for c in "loop".chars() {
			app.handle_key(plain(KeyCode::Char(c)));
		}
		match &app.modal {
			Some(Modal::Select(dialog)) => assert_eq!(dialog.selector.filter(), "loop"),
			other => panic!("unexpected modal {other:?}"),
		}
		assert_eq!(app.handle_key(plain(KeyCode::Enter)), Some(Request::Apply(Choice::ConnectMidi(1))));
		assert!(app.modal.is_none());

		app.open_dialog(midi_dialog());
		assert_eq!(app.handle_key(plain(KeyCode::Esc)), None);
		assert!(app.modal.is_none());
	}

	#[test]
	fn sf2_dialog_submits_a_path_and_stays_open_until_fulfilled() {
		let mut app = App::new("en");
		app.open_modal(Modal::Sf2Path(PathInput::new("", vec![])));
		app.handle_paste("assets/sin_wave.sf2");
		assert_eq!(
			app.handle_key(plain(KeyCode::Enter)),
			Some(Request::LoadBank(PathBuf::from("assets/sin_wave.sf2")))
		);
		assert!(app.modal.is_some(), "closed only by fulfil on success");
		assert_eq!(app.handle_key(plain(KeyCode::Esc)), None);
		assert!(app.modal.is_none());
	}

	#[test]
	fn paste_outside_the_path_dialog_is_ignored() {
		let mut app = App::new("en");
		app.take_dirty();
		app.handle_paste("ignored");
		assert!(!app.take_dirty());
	}

	#[test]
	fn monitor_keys_work_only_while_the_monitor_panel_is_focused() {
		use crate::midi::{MidiEvent, MonitorEntry};
		let mut app = App::new("en");
		app.monitor
			.extend((0..5).map(|key| MonitorEntry::now(MidiEvent::NoteOff { channel: 0, key })));
		app.handle_key(plain(KeyCode::Up));
		assert_eq!(app.monitor.scroll(), 1);
		app.handle_key(plain(KeyCode::PageUp));
		assert_eq!(app.monitor.scroll(), 4, "clamped to the oldest entry");
		app.handle_key(plain(KeyCode::End));
		assert_eq!(app.monitor.scroll(), 0);
		app.handle_key(press(KeyCode::Char('f'), KeyModifiers::CONTROL));
		assert!(app.monitor.is_paused());
		app.handle_key(plain(KeyCode::Tab));
		app.handle_key(press(KeyCode::Char('l'), KeyModifiers::CONTROL));
		assert_eq!(app.monitor.len(), 5, "Ctrl+L ignored while the mixer is focused");
		app.handle_key(plain(KeyCode::Tab));
		app.handle_key(press(KeyCode::Char('l'), KeyModifiers::CONTROL));
		assert!(app.monitor.is_empty());
	}

	#[test]
	fn f1_opens_help_which_scrolls_and_closes() {
		let mut app = App::new("en");
		assert_eq!(app.handle_key(plain(KeyCode::F(1))), None);
		assert_eq!(app.modal, Some(Modal::Help(0)));
		app.handle_key(plain(KeyCode::Down));
		app.handle_key(plain(KeyCode::PageDown));
		assert_eq!(app.modal, Some(Modal::Help(11)));
		app.handle_key(plain(KeyCode::Up));
		assert_eq!(app.modal, Some(Modal::Help(10)));
		assert_eq!(app.handle_key(plain(KeyCode::F(3))), None, "hotkeys do not leak through the help");
		app.handle_key(plain(KeyCode::F(1)));
		assert!(app.modal.is_none());
		app.handle_key(plain(KeyCode::F(1)));
		app.handle_key(plain(KeyCode::Esc));
		assert!(app.modal.is_none());
	}
}
