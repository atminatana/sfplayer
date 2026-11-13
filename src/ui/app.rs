//! Application state and key handling, independent of the window that shows it.
//!
//! `App` never touches devices: key handling yields a `Request`, which `fulfil` applies to the `Session`.
//! That keeps dialogs and hotkeys testable without hardware.

use std::time::{Duration, Instant};

use super::chord::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::dialog::{Choice, Dialog, DialogKind};
use super::effects_dialog::{EffectsDialog, EffectsOutcome};
use super::keys::{Action, Bindings};
use super::meter::ClipHold;
use super::path_input::{InputKind, PathRequest};
use super::selector::SelectorOutcome;
use super::text_keyboard::TextKeyboard;
use crate::audio;
use crate::config::{AdsrField, EngineKind, KeyboardConfig, RecordFormat};
use crate::midi::MidiEvent;
use crate::session::{EffectsAction, EffectsSettings, MixerAction, Notice, Session};
use crate::synth::{AdsrPreset, Wave};

/// Spec §6: redraw at most 30 frames per second.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// MIDI channel the text keyboard and the on-screen piano play on (0-based): channel 1, where the F5
/// preset applies (ADR 0011).
pub const KEYBOARD_CHANNEL: u8 = 0;

/// What the UI wants the session to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
	Mixer(MixerAction),
	OpenDialog(DialogKind),
	OpenSf2Dialog,
	/// The MIDI file (spec v1.2, F-21): the dialog that chooses one, and the transport.
	OpenMidiFileDialog,
	FilePlay,
	FilePause,
	FilePlayPause,
	FileStop,
	OpenEffectsDialog,
	OpenRecordDialog,
	Apply(Choice),
	/// A format button of the main screen.
	SetRecordFormat(RecordFormat),
	/// The processor buttons and F12 (spec v1.2, F-22).
	SetEngine(EngineKind),
	ToggleEngine,
	/// The timbre of the built-in synthesizer: the wave, a ready-made envelope, one value a step
	/// down (-1) or up (+1).
	SetWave(Wave),
	SetAdsrPreset(AdsrPreset),
	AdjustAdsr(AdsrField, i8),
	Rescan,
	Panic,
	/// Home / End (spec v1.1 §4.3).
	StartRecord,
	StopRecord,
	/// Quit confirmed while recording: stop the take, then quit.
	StopRecordingAndQuit,
	/// Text keyboard output: goes to the engine through the command ring (ADR 0009).
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
	Effects(EffectsDialog),
	/// F1: scroll offset in rows.
	Help(usize),
	/// F10 / Ctrl+C while recording (spec §6): Enter stops and quits, Esc stays.
	ConfirmQuit,
}

/// The `[buttons]` of the main screen (spec v1.2 §3), in the order Tab walks them: top to bottom,
/// left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
	MidiIn,
	MidiFile,
	FileStart,
	FilePause,
	FileStop,
	EngineSynth,
	EngineSf2,
	WaveSine,
	WaveSaw,
	WaveTriangle,
	AdsrPiano,
	AdsrHarpsichord,
	AdsrOrgan,
	AdsrFlute,
	AttackDown,
	AttackUp,
	DecayDown,
	DecayUp,
	SustainDown,
	SustainUp,
	ReleaseDown,
	ReleaseUp,
	Sf2File,
	Preset,
	Effects,
	AudioOut,
	MasterDown,
	MasterUp,
	Folder,
	FormatFlac,
	FormatOgg,
	FormatMidi,
	RecordStart,
	RecordStop,
	RecordCancel,
}

impl ButtonId {
	pub const ALL: [Self; 35] = [
		Self::MidiIn,
		Self::MidiFile,
		Self::FileStart,
		Self::FilePause,
		Self::FileStop,
		Self::EngineSynth,
		Self::EngineSf2,
		Self::WaveSine,
		Self::WaveSaw,
		Self::WaveTriangle,
		Self::AdsrPiano,
		Self::AdsrHarpsichord,
		Self::AdsrOrgan,
		Self::AdsrFlute,
		Self::AttackDown,
		Self::AttackUp,
		Self::DecayDown,
		Self::DecayUp,
		Self::SustainDown,
		Self::SustainUp,
		Self::ReleaseDown,
		Self::ReleaseUp,
		Self::Sf2File,
		Self::Preset,
		Self::Effects,
		Self::AudioOut,
		Self::MasterDown,
		Self::MasterUp,
		Self::Folder,
		Self::FormatFlac,
		Self::FormatOgg,
		Self::FormatMidi,
		Self::RecordStart,
		Self::RecordStop,
		Self::RecordCancel,
	];
}

pub struct App {
	pub locale: &'static str,
	pub should_quit: bool,
	pub modal: Option<Modal>,
	/// A path the user is to be asked for; the window takes it and shows the input dialog.
	pub path_request: Option<PathRequest>,
	pub keyboard: TextKeyboard,
	pub bindings: Bindings,
	/// Elapsed time of the running recording, refreshed every frame; `None` when not recording.
	pub recording: Option<Duration>,
	clip_hold: ClipHold,
	/// Whether the CLIP mark is lit for the frame being drawn.
	pub clipping: bool,
	dirty: bool,
}

impl App {
	pub fn new(locale: &'static str) -> Self {
		Self::with_keyboard(locale, TextKeyboard::new(&KeyboardConfig::default()))
	}

	pub fn with_keyboard(locale: &'static str, keyboard: TextKeyboard) -> Self {
		Self {
			locale,
			should_quit: false,
			modal: None,
			path_request: None,
			keyboard,
			bindings: Bindings::default(),
			recording: None,
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
		let action = self.bindings.action_for(&key);
		if action == Some(Action::Quit) {
			self.request_quit();
			return None;
		}
		if self.modal.is_some() {
			self.dirty = true;
			return self.handle_modal_key(key);
		}
		match action {
			Some(action) => self.perform(action),
			None => self.handle_note_key(key),
		}
	}

	/// Quitting while a take runs asks first (spec §6); asked again, it quits.
	fn request_quit(&mut self) {
		self.dirty = true;
		if self.recording.is_some() && !matches!(self.modal, Some(Modal::ConfirmQuit)) {
			self.modal = Some(Modal::ConfirmQuit);
			return;
		}
		self.should_quit = true;
	}

	/// A button of the main screen pressed with the mouse or with Enter: the same commands the hotkeys give.
	pub fn press_button(&mut self, button: ButtonId) -> Option<Request> {
		match button {
			ButtonId::MidiIn => self.perform(Action::MidiDialog),
			ButtonId::Sf2File => self.perform(Action::Sf2Dialog),
			ButtonId::Preset => self.perform(Action::PresetDialog),
			ButtonId::Effects => self.perform(Action::EffectsDialog),
			ButtonId::AudioOut => self.perform(Action::AudioDialog),
			// The master buttons step by 5 % (spec v1.2, F-10).
			ButtonId::MasterDown => self.perform(Action::MasterDownFast),
			ButtonId::MasterUp => self.perform(Action::MasterUpFast),
			ButtonId::Folder => self.perform(Action::RecordDialog),
			ButtonId::FormatFlac => self.choose_format(RecordFormat::Flac),
			ButtonId::FormatOgg => self.choose_format(RecordFormat::OggVorbis),
			ButtonId::RecordStart => self.perform(Action::RecordStart),
			ButtonId::RecordStop => self.perform(Action::RecordStop),
			ButtonId::EngineSynth => self.ask(Request::SetEngine(EngineKind::Builtin)),
			ButtonId::EngineSf2 => self.ask(Request::SetEngine(EngineKind::Sf2)),
			ButtonId::WaveSine => self.ask(Request::SetWave(Wave::Sine)),
			ButtonId::WaveSaw => self.ask(Request::SetWave(Wave::Saw)),
			ButtonId::WaveTriangle => self.ask(Request::SetWave(Wave::Triangle)),
			ButtonId::AdsrPiano => self.ask(Request::SetAdsrPreset(AdsrPreset::Piano)),
			ButtonId::AdsrHarpsichord => self.ask(Request::SetAdsrPreset(AdsrPreset::Harpsichord)),
			ButtonId::AdsrOrgan => self.ask(Request::SetAdsrPreset(AdsrPreset::Organ)),
			ButtonId::AdsrFlute => self.ask(Request::SetAdsrPreset(AdsrPreset::Flute)),
			ButtonId::AttackDown => self.ask(Request::AdjustAdsr(AdsrField::Attack, -1)),
			ButtonId::AttackUp => self.ask(Request::AdjustAdsr(AdsrField::Attack, 1)),
			ButtonId::DecayDown => self.ask(Request::AdjustAdsr(AdsrField::Decay, -1)),
			ButtonId::DecayUp => self.ask(Request::AdjustAdsr(AdsrField::Decay, 1)),
			ButtonId::SustainDown => self.ask(Request::AdjustAdsr(AdsrField::Sustain, -1)),
			ButtonId::SustainUp => self.ask(Request::AdjustAdsr(AdsrField::Sustain, 1)),
			ButtonId::ReleaseDown => self.ask(Request::AdjustAdsr(AdsrField::Release, -1)),
			ButtonId::ReleaseUp => self.ask(Request::AdjustAdsr(AdsrField::Release, 1)),
			ButtonId::MidiFile => self.perform(Action::MidiFileDialog),
			ButtonId::FileStart => self.ask(Request::FilePlay),
			ButtonId::FilePause => self.ask(Request::FilePause),
			ButtonId::FileStop => self.perform(Action::FileStop),
			// Recording MIDI and cancelling a take arrive with plan stage 6; until then the screen shows
			// these buttons unavailable.
			ButtonId::FormatMidi | ButtonId::RecordCancel => None,
		}
	}

	/// A request that changes what the screen shows.
	fn ask(&mut self, request: Request) -> Option<Request> {
		self.dirty = true;
		Some(request)
	}

	/// The format cannot change under a running take (spec v1.2, F-23).
	fn choose_format(&mut self, format: RecordFormat) -> Option<Request> {
		if self.recording.is_some() {
			return None;
		}
		self.dirty = true;
		Some(Request::SetRecordFormat(format))
	}

	/// Carries out a command, whichever way it was given: a hotkey or a button.
	fn perform(&mut self, action: Action) -> Option<Request> {
		self.dirty = true;
		match action {
			Action::Quit => {
				self.request_quit();
				None
			}
			Action::Help => {
				self.modal = Some(Modal::Help(0));
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
			Action::EngineToggle => Some(Request::ToggleEngine),
			Action::MidiFileDialog => Some(Request::OpenMidiFileDialog),
			Action::FilePlayPause => Some(Request::FilePlayPause),
			Action::FileStop => Some(Request::FileStop),
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
		}
	}

	/// A key that is not a command: a note on the text keyboard when it is a plain (or shifted) character.
	fn handle_note_key(&mut self, key: KeyEvent) -> Option<Request> {
		let KeyCode::Char(c) = key.code else {
			return None;
		};
		if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) || key.kind == KeyEventKind::Repeat {
			return None;
		}
		let events = self.keyboard.press(c, KEYBOARD_CHANNEL);
		self.notes(events)
	}

	/// Key release events.
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
		}
	}

	/// A click outside the open dialog closes it the way Esc does (spec v1.2 §4.1).
	pub fn dismiss_modal(&mut self) -> Option<Request> {
		self.modal.as_ref()?;
		self.dirty = true;
		self.handle_modal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
	}

	pub fn open_modal(&mut self, modal: Modal) {
		self.modal = Some(modal);
		self.dirty = true;
	}

	pub fn open_dialog(&mut self, dialog: Dialog) {
		self.open_modal(Modal::Select(dialog));
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
				app.open_dialog(Dialog::preset(loaded.bank.presets(), session.preset_for()));
			}
		}
		Request::OpenRecordDialog => {
			let record = &session.config.record;
			app.path_request = Some(PathRequest {
				kind: InputKind::RecordDir,
				initial: record
					.dir
					.as_ref()
					.map(|dir| dir.to_string_lossy().into_owned())
					.unwrap_or_default(),
				history: record.recent_dirs.clone(),
			});
		}
		Request::OpenEffectsDialog => app.open_modal(Modal::Effects(EffectsDialog::new(session.effects()))),
		Request::OpenSf2Dialog => {
			let initial = session
				.bank
				.as_ref()
				.map(|loaded| loaded.path.to_string_lossy().into_owned())
				.unwrap_or_default();
			app.path_request = Some(PathRequest {
				kind: InputKind::Sf2File,
				initial,
				history: session.config.synth.recent_banks.clone(),
			});
		}
		Request::OpenMidiFileDialog => {
			let source = &session.config.source;
			app.path_request = Some(PathRequest {
				kind: InputKind::MidiFile,
				initial: source
					.midi_file
					.as_ref()
					.map(|path| path.to_string_lossy().into_owned())
					.unwrap_or_default(),
				history: source.recent_midi_files.clone(),
			});
		}
		Request::FilePlay => session.file_play(),
		Request::FilePause => session.file_pause(),
		Request::FilePlayPause => session.file_play_pause(),
		Request::FileStop => session.file_stop(),
		Request::Apply(Choice::ConnectMidi(index)) => {
			session.connect_midi(index);
			session.save_config();
		}
		Request::Apply(Choice::OpenAudio(index)) => {
			session.reopen_audio(index);
			session.save_config();
		}
		Request::Apply(Choice::SelectPreset { bank, program }) => {
			session.select_preset(bank, program);
			session.save_config();
		}
		Request::SetRecordFormat(format) => {
			session.set_record_format(format);
			session.save_config();
		}
		// The engine changes under the keys: the notes the text keyboard holds end with the old one.
		Request::SetEngine(kind) => {
			app.keyboard.clear();
			session.set_engine(kind);
			session.save_config();
		}
		Request::ToggleEngine => {
			app.keyboard.clear();
			session.set_engine(session.config.synth.engine.other());
			session.save_config();
		}
		// The timbre is written with the rest of the config when the program closes, like the master
		// volume: these are stepped many times in a row.
		Request::SetWave(wave) => session.set_wave(wave),
		Request::SetAdsrPreset(preset) => session.set_adsr_preset(preset),
		Request::AdjustAdsr(field, steps) => session.adjust_adsr(field, steps),
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
			tracing::debug!(?events, "notes to the command ring");
			session.send_notes(&events);
		}
	}
}

#[cfg(test)]
mod tests;
