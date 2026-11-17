//! Application state and key handling, independent of the window that shows it.
//!
//! `App` never touches devices: key handling yields a `Request`, which `fulfil` applies to the `Session`.
//! That keeps dialogs and hotkeys testable without hardware.

use std::time::{Duration, Instant};

use super::chord::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::dialog::{Choice, PresetRequest};
use super::keys::{Action, Bindings};
use super::meter::ClipHold;
use super::path_input::PathRequest;
use super::text_keyboard::TextKeyboard;
use crate::config::{AdsrField, EngineKind, KeyboardConfig, RecordFormat};
use crate::midi::MidiEvent;
use crate::session::{EffectsAction, EffectsSettings, MixerAction};
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
	/// F5 and the Preset button: the list of presets of the loaded bank.
	OpenPresetDialog,
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
	/// A row of the format dropdown of the main screen.
	SetRecordFormat(RecordFormat),
	/// The processor buttons and the `engine_toggle` command (spec v1.2, F-22).
	SetEngine(EngineKind),
	ToggleEngine,
	/// The timbre of the built-in synthesizer: the wave, a ready-made envelope, one value a step
	/// down (-1) or up (+1).
	SetWave(Wave),
	SetAdsrPreset(AdsrPreset),
	AdjustAdsr(AdsrField, i8),
	Rescan,
	Panic,
	/// The Start and Stop buttons of a take (spec v1.1 §4.3).
	StartRecord,
	StopRecord,
	/// The take is thrown away: the Cancel button, once confirmed (spec v1.2 F-23, v1.3 F-27).
	CancelRecord,
	/// Quit confirmed while recording: stop the take, then quit.
	StopRecordingAndQuit,
	/// Text keyboard output: goes to the engine through the command ring (ADR 0009).
	Notes(Vec<MidiEvent>),
	/// Semitone delta for `[` / `]`.
	Transpose(i8),
	Effects(EffectsAction),
	/// `[OK]` in the effects window: apply and write the config.
	SaveEffects(EffectsSettings),
}

/// What the confirmation window asks before something a key press does not take back (spec v1.3,
/// F-26.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
	/// The quit command while a take runs (spec §6): stop the take and quit?
	Quit,
	/// The Cancel button of a take (spec v1.3, F-27): throw the take away?
	DeleteTake,
}

/// The `[buttons]` of the main screen (spec v1.2 §3), in the order Tab walks them: top to bottom,
/// left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
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
	MasterDown,
	MasterUp,
	Folder,
	RecordStart,
	RecordStop,
	RecordCancel,
}

impl ButtonId {
	pub const ALL: [Self; 30] = [
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
		Self::MasterDown,
		Self::MasterUp,
		Self::Folder,
		Self::RecordStart,
		Self::RecordStop,
		Self::RecordCancel,
	];
}

/// The dropdowns of the main screen, in the order of the screen: the two devices (spec v1.3, F-26.1)
/// and the format of takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropdown {
	MidiIn,
	AudioOut,
	Format,
}

impl Dropdown {
	pub const ALL: [Self; 3] = [Self::MidiIn, Self::AudioOut, Self::Format];
}

/// The formats of a take in the order of their dropdown, and what each is called there.
pub const RECORD_FORMATS: [(RecordFormat, &str); 3] = [
	(RecordFormat::Flac, "FLAC"),
	(RecordFormat::OggVorbis, "OGG"),
	(RecordFormat::Midi, "MIDI"),
];

pub struct App {
	pub locale: &'static str,
	pub should_quit: bool,
	/// The question the confirmation window is asking now. The window shows it with controls and
	/// reports the answer through `answer`.
	pub confirm: Option<Question>,
	/// A path the user is to be asked for; the window takes it and shows the input dialog.
	pub path_request: Option<PathRequest>,
	/// A dropdown of the main screen asked for by its key; the window takes it and drops the list
	/// down.
	pub dropdown_request: Option<Dropdown>,
	/// The presets the user is to choose from; the window takes it and shows the preset window.
	pub preset_request: Option<PresetRequest>,
	/// The effects the user is to adjust; the window takes it and shows the effects window.
	pub effects_request: Option<EffectsSettings>,
	/// The help was asked for; the window takes the flag and shows the help window.
	pub is_help_requested: bool,
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
			confirm: None,
			path_request: None,
			dropdown_request: None,
			preset_request: None,
			effects_request: None,
			is_help_requested: false,
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

	/// The elapsed time of the running take, refreshed on every tick. A take that ended by itself
	/// takes the question about deleting it along: there is nothing left to delete.
	pub fn set_recording(&mut self, recording: Option<Duration>) {
		if recording.is_none() && self.confirm == Some(Question::DeleteTake) {
			self.confirm = None;
			self.dirty = true;
		}
		self.recording = recording;
	}

	/// Handles one key press on the main screen: the keys of an open window go to the window and do
	/// not come here. Quit works everywhere.
	pub fn handle_key(&mut self, key: KeyEvent) -> Option<Request> {
		let action = self.bindings.action_for(&key);
		if action == Some(Action::Quit) {
			self.request_quit();
			return None;
		}
		// The keys of an open question belong to the controls of its window.
		if self.confirm.is_some() {
			return None;
		}
		match action {
			Some(action) => self.perform(action),
			None => self.handle_note_key(key),
		}
	}

	/// Quitting while a take runs asks first (spec §6); asked again, it quits.
	fn request_quit(&mut self) {
		self.dirty = true;
		if self.recording.is_some() && self.confirm != Some(Question::Quit) {
			self.confirm = Some(Question::Quit);
			return;
		}
		self.should_quit = true;
	}

	/// The answer to the open question, from its window: a confirmation does what was asked about.
	pub fn answer(&mut self, is_confirmed: bool) -> Option<Request> {
		let question = self.confirm.take()?;
		self.dirty = true;
		if !is_confirmed {
			return None;
		}
		match question {
			Question::Quit => {
				self.should_quit = true;
				Some(Request::StopRecordingAndQuit)
			}
			Question::DeleteTake => Some(Request::CancelRecord),
		}
	}

	/// A button of the main screen pressed with the mouse or with Enter: the same commands the hotkeys give.
	pub fn press_button(&mut self, button: ButtonId) -> Option<Request> {
		match button {
			ButtonId::Sf2File => self.perform(Action::Sf2Dialog),
			ButtonId::Preset => self.perform(Action::PresetDialog),
			ButtonId::Effects => self.perform(Action::EffectsDialog),
			// The master buttons step by 5 % (spec v1.2, F-10).
			ButtonId::MasterDown => self.perform(Action::MasterDownFast),
			ButtonId::MasterUp => self.perform(Action::MasterUpFast),
			ButtonId::Folder => self.perform(Action::RecordDialog),
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
			// Throwing a take away is asked about first (spec v1.3, F-27).
			ButtonId::RecordCancel => {
				self.dirty = true;
				if self.recording.is_some() {
					self.confirm = Some(Question::DeleteTake);
				}
				None
			}
		}
	}

	/// A request that changes what the screen shows.
	fn ask(&mut self, request: Request) -> Option<Request> {
		self.dirty = true;
		Some(request)
	}

	/// A row of the format dropdown was chosen. The format cannot change under a running take
	/// (spec v1.2, F-23).
	pub fn choose_format(&mut self, row: usize) -> Option<Request> {
		if self.recording.is_some() {
			return None;
		}
		let (format, _) = RECORD_FORMATS.get(row)?;
		self.dirty = true;
		Some(Request::SetRecordFormat(*format))
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
				self.is_help_requested = true;
				None
			}
			// The devices are chosen from dropdowns of the screen: their keys ask the window to drop
			// the list down.
			Action::MidiDialog => {
				self.dropdown_request = Some(Dropdown::MidiIn);
				None
			}
			Action::AudioDialog => {
				self.dropdown_request = Some(Dropdown::AudioOut);
				None
			}
			Action::Sf2Dialog => Some(Request::OpenSf2Dialog),
			Action::PresetDialog => Some(Request::OpenPresetDialog),
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
		if self.confirm.is_some() {
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

	/// Returns and clears the redraw flag.
	pub fn take_dirty(&mut self) -> bool {
		std::mem::take(&mut self.dirty)
	}
}

#[cfg(test)]
mod tests;
