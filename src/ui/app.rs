//! Application state and the event loop (≤ 30 frames/s, never blocks the audio thread).
//!
//! `App` never touches devices: key handling yields a `Request`, which `fulfil` applies to the `Session`.
//! That keeps dialogs and hotkeys testable without hardware.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::dialog::{Choice, Dialog, DialogKind};
use super::effects_dialog::{EffectsDialog, EffectsOutcome};
use super::glyphs::Glyphs;
use super::keys::{Action, Bindings};
use super::meter::ClipHold;
use super::monitor::Monitor;
use super::path_input::{self, PathInput, PathOutcome};
use super::record_dialog::{RecordDialog, RecordOutcome};
use super::screen;
use super::selector::SelectorOutcome;
use super::text_keyboard::{NoteOffMode, TextKeyboard};
use crate::audio;
use crate::config::KeyboardConfig;
use crate::midi::{MidiEvent, MonitorEntry};
use crate::session::{EffectsAction, EffectsSettings, MixerAction, Notice, Session};

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
	Mixer(MixerAction),
	OpenDialog(DialogKind),
	OpenSf2Dialog,
	OpenEffectsDialog,
	OpenRecordDialog,
	Apply(Choice),
	LoadBank(PathBuf),
	Rescan,
	Panic,
	/// Home / End (spec v1.1 §4.3).
	StartRecord,
	StopRecord,
	/// Quit confirmed while recording: stop the take, then quit.
	StopRecordingAndQuit,
	/// Text keyboard output: goes to the engine and to the monitor.
	Notes(Vec<MidiEvent>),
	/// Semitone delta for `[` / `]`.
	Transpose(i8),
	Effects(EffectsAction),
	/// Enter in the F6 dialog: apply and write the config.
	SaveEffects(EffectsSettings),
}

/// The modal window on top of the main screen, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
	Select(Dialog),
	Sf2Path(PathInput),
	Effects(EffectsDialog),
	Record(RecordDialog),
	/// F1: scroll offset in rows.
	Help(usize),
	/// F10 / Ctrl+C while recording (spec §6): Enter stops and quits, Esc stays.
	ConfirmQuit,
}

pub struct App {
	pub locale: &'static str,
	pub focus: Panel,
	pub should_quit: bool,
	pub modal: Option<Modal>,
	pub monitor: Monitor,
	pub keyboard: TextKeyboard,
	pub bindings: Bindings,
	pub glyphs: Glyphs,
	/// Elapsed time of the running recording, refreshed every frame; `None` when not recording.
	pub recording: Option<Duration>,
	/// MIDI channel the text keyboard plays on (0-based); follows the mixer's current channel.
	pub keyboard_channel: u8,
	clip_hold: ClipHold,
	/// Whether the CLIP mark is lit for the frame being drawn.
	pub clipping: bool,
	dirty: bool,
}

impl App {
	pub fn new(locale: &'static str) -> Self {
		Self::with_keyboard(
			locale,
			TextKeyboard::new(&KeyboardConfig::default(), NoteOffMode::Gate(Duration::from_millis(300))),
		)
	}

	pub fn with_keyboard(locale: &'static str, keyboard: TextKeyboard) -> Self {
		Self {
			locale,
			focus: Panel::Monitor,
			should_quit: false,
			modal: None,
			monitor: Monitor::default(),
			keyboard,
			bindings: Bindings::default(),
			glyphs: Glyphs::UNICODE,
			recording: None,
			keyboard_channel: 0,
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
		if self.bindings.action_for(&key) == Some(Action::Quit) {
			self.dirty = true;
			if self.recording.is_some() && !matches!(self.modal, Some(Modal::ConfirmQuit)) {
				self.modal = Some(Modal::ConfirmQuit);
				return None;
			}
			self.should_quit = true;
			return None;
		}
		if self.modal.is_some() {
			self.dirty = true;
			return self.handle_modal_key(key);
		}
		let Some(action) = self.bindings.action_for(&key) else {
			return self.handle_note_key(key);
		};
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
			Action::EffectsDialog => Some(Request::OpenEffectsDialog),
			Action::RecordDialog => Some(Request::OpenRecordDialog),
			Action::Rescan => Some(Request::Rescan),
			Action::RecordStart => Some(Request::StartRecord),
			Action::RecordStop => Some(Request::StopRecord),
			Action::Panic => {
				self.keyboard.clear();
				Some(Request::Panic)
			}
			Action::OctaveUp => {
				self.keyboard.octave_up();
				None
			}
			Action::OctaveDown => {
				self.keyboard.octave_down();
				None
			}
			Action::VelocityUp => {
				self.keyboard.velocity_up();
				None
			}
			Action::VelocityDown => {
				self.keyboard.velocity_down();
				None
			}
			Action::ToggleReverb => Some(Request::Effects(EffectsAction::ToggleReverb)),
			Action::ToggleChorus => Some(Request::Effects(EffectsAction::ToggleChorus)),
			Action::TransposeUp => Some(Request::Transpose(1)),
			Action::TransposeDown => Some(Request::Transpose(-1)),
			Action::NotesOff => {
				let events = self.keyboard.all_notes_off();
				self.notes(events)
			}
			Action::MasterUp => Some(Request::Mixer(MixerAction::MasterUp)),
			Action::MasterDown => Some(Request::Mixer(MixerAction::MasterDown)),
			Action::MasterUpFast => Some(Request::Mixer(MixerAction::MasterUpFast)),
			Action::MasterDownFast => Some(Request::Mixer(MixerAction::MasterDownFast)),
			Action::MasterMute => Some(Request::Mixer(MixerAction::MasterMute)),
			Action::SelectChannel(_)
			| Action::ChannelUp
			| Action::ChannelDown
			| Action::ChannelMute
			| Action::ChannelSolo
			| Action::MonitorClear
			| Action::MonitorPause
			| Action::ScrollUp
			| Action::ScrollDown
			| Action::PageUp
			| Action::PageDown => self.handle_panel_action(action),
		}
	}

	/// Keys whose meaning depends on the focused panel (spec §6 "контекст").
	fn handle_panel_action(&mut self, action: Action) -> Option<Request> {
		match (self.focus, action) {
			(Panel::Monitor, Action::MonitorClear) => self.monitor.clear(),
			(Panel::Monitor, Action::MonitorPause) => self.monitor.toggle_pause(),
			(Panel::Monitor, Action::ScrollUp) => self.monitor.scroll_up(1),
			(Panel::Monitor, Action::ScrollDown) => self.monitor.scroll_down(1),
			(Panel::Monitor, Action::PageUp) => self.monitor.scroll_up(MONITOR_PAGE),
			(Panel::Monitor, Action::PageDown) => self.monitor.scroll_down(MONITOR_PAGE),
			(Panel::Mixer, Action::SelectChannel(channel)) => return Some(Request::Mixer(MixerAction::SelectChannel(channel))),
			(Panel::Mixer, Action::ChannelUp) => return Some(Request::Mixer(MixerAction::ChannelUp)),
			(Panel::Mixer, Action::ChannelDown) => return Some(Request::Mixer(MixerAction::ChannelDown)),
			(Panel::Mixer, Action::ChannelMute) => return Some(Request::Mixer(MixerAction::ChannelMute)),
			(Panel::Mixer, Action::ChannelSolo) => return Some(Request::Mixer(MixerAction::ChannelSolo)),
			_ => {}
		}
		None
	}

	/// A key that is not a command: a note on the text keyboard when it is a plain (or shifted) character.
	fn handle_note_key(&mut self, key: KeyEvent) -> Option<Request> {
		let KeyCode::Char(c) = key.code else {
			return None;
		};
		if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) || key.kind == KeyEventKind::Repeat {
			return None;
		}
		let events = self.keyboard.press(c, self.keyboard_channel, Instant::now());
		self.notes(events)
	}

	/// Key release events (only delivered by terminals that report them).
	pub fn handle_release(&mut self, key: KeyEvent) -> Option<Request> {
		if self.modal.is_some() {
			return None;
		}
		let KeyCode::Char(c) = key.code else {
			return None;
		};
		let events = self.keyboard.release(c);
		self.notes(events)
	}

	/// Gate timer; called every frame.
	pub fn tick_keyboard(&mut self, now: Instant) -> Option<Request> {
		let events = self.keyboard.tick(now);
		self.notes(events)
	}

	fn notes(&mut self, events: Vec<MidiEvent>) -> Option<Request> {
		if events.is_empty() {
			return None;
		}
		self.dirty = true;
		Some(Request::Notes(events))
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
			Modal::ConfirmQuit => match key.code {
				KeyCode::Enter => {
					self.modal = None;
					self.should_quit = true;
					Some(Request::StopRecordingAndQuit)
				}
				KeyCode::Esc => {
					self.modal = None;
					None
				}
				_ => None,
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
			Modal::Effects(dialog) => match dialog.handle_key(key) {
				EffectsOutcome::Pending => None,
				EffectsOutcome::Changed(settings) => Some(Request::Effects(EffectsAction::Set(settings))),
				EffectsOutcome::Accepted(settings) => {
					self.modal = None;
					Some(Request::SaveEffects(settings))
				}
				EffectsOutcome::Cancelled(rollback) => {
					self.modal = None;
					rollback.map(|settings| Request::Effects(EffectsAction::Set(settings)))
				}
			},
			Modal::Record(dialog) => match dialog.handle_key(key, path_input::list_dir) {
				RecordOutcome::Pending => None,
				RecordOutcome::Cancelled => {
					self.modal = None;
					None
				}
				RecordOutcome::Accepted { dir, format } => {
					self.modal = None;
					Some(Request::Apply(Choice::SetRecording { dir, format }))
				}
			},
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
		match self.modal.as_mut() {
			Some(Modal::Sf2Path(input)) => input.insert_str(text),
			Some(Modal::Record(dialog)) => dialog.insert_str(text),
			_ => return,
		}
		self.dirty = true;
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
		Request::Mixer(action) => session.apply_mixer(action),
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
		Request::OpenRecordDialog => app.open_modal(Modal::Record(RecordDialog::new(
			session.config.record.dir.as_deref(),
			session.config.record.format,
			session.config.record.vorbis_quality,
		))),
		Request::OpenEffectsDialog => app.open_modal(Modal::Effects(EffectsDialog::new(session.effects()))),
		Request::OpenSf2Dialog => {
			let initial = session
				.bank
				.as_ref()
				.map(|loaded| loaded.path.to_string_lossy().into_owned())
				.unwrap_or_default();
			app.open_modal(Modal::Sf2Path(PathInput::new(&initial, session.config.synth.recent_banks.clone())));
		}
		Request::Apply(Choice::ConnectMidi(index)) => {
			session.connect_midi(index);
			session.save_config();
		}
		Request::Apply(Choice::OpenAudio(index)) => {
			session.reopen_audio(index);
			session.save_config();
		}
		Request::Apply(Choice::SelectPreset { bank, program }) => {
			let channel = session.current_channel;
			session.select_preset(channel, bank, program);
		}
		Request::Apply(Choice::SetRecording { dir, format }) => {
			session.set_recording(dir, format);
			session.save_config();
		}
		Request::LoadBank(path) => match session.load_bank(&path) {
			Ok(()) => {
				app.modal = None;
				session.save_config();
			}
			Err(error) => {
				if let Some(Modal::Sf2Path(input)) = app.modal.as_mut() {
					input.set_error(error.to_string());
				}
			}
		},
		Request::Rescan => session.rescan(),
		Request::StartRecord => session.start_recording(),
		Request::StopRecord | Request::StopRecordingAndQuit => session.stop_recording(),
		Request::Panic => session.panic(),
		Request::Transpose(delta) => session.adjust_transpose(delta),
		Request::Effects(action) => session.apply_effects(action),
		Request::SaveEffects(settings) => {
			session.apply_effects(EffectsAction::Set(settings));
			session.save_config();
		}
		Request::Notes(events) => {
			session.send_notes(&events);
			app.monitor.extend(events.into_iter().map(MonitorEntry::now));
		}
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
		app.keyboard_channel = session.current_channel;
		app.recording = session.recording_elapsed();
		if let Some(request) = app.tick_keyboard(now) {
			fulfil(request, app, session);
		}
		app.update_meters(session.shared.clips(), now);
		if app.take_dirty() || now >= next_frame {
			terminal.draw(|frame| screen::render(frame, app, &screen::View::from_session(session)))?;
			next_frame = now + FRAME_INTERVAL;
		}
		let timeout = next_frame.saturating_duration_since(Instant::now());
		if event::poll(timeout)? {
			match event::read()? {
				Event::Key(key) if key.kind == KeyEventKind::Release => {
					if let Some(request) = app.handle_release(key) {
						fulfil(request, app, session);
					}
				}
				Event::Key(key) => {
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
mod tests;
