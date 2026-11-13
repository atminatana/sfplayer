//! What the screen shows, decoupled from live handles and from the drawing backend, so that rendering is
//! testable (spec v1.1 §2; plan v1.2 step 1.2). Also the texts that depend only on this model: which
//! line a notice belongs to and how it reads.

use cpal::BufferSize;
use rust_i18n::t;

use super::dialog::preset_label;
use crate::audio::{Levels, Mixer};
use crate::config::RecordFormat;
use crate::midi::MidiStatus;
use crate::session::{Notice, Session};

/// Everything the screen shows, decoupled from live handles so rendering is testable.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
	pub midi: MidiStatus,
	pub audio: Option<AudioView>,
	pub bank: Option<BankView>,
	/// Reverb / chorus send levels; `None` = off (F-13).
	pub reverb: Option<u8>,
	pub chorus: Option<u8>,
	/// Where takes go (resolved, even when `record.dir` is unset) and in which format.
	pub record_dir: String,
	pub record_format: RecordFormat,
	pub xruns: u32,
	pub dropped: u32,
	pub transpose: i8,
	pub omni_channel: Option<u8>,
	pub record_dropped: u32,
	pub held_notes: u32,
	pub max_voices: u32,
	pub load_percent: u32,
	pub levels: Levels,
	pub mixer: Mixer,
	pub notice: Option<Notice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioView {
	pub device: String,
	pub sample_rate: u32,
	/// Buffer size in frames; `None` when the device picked its own default.
	pub buffer: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankView {
	/// File name of the loaded bank (spec v1.1 §2: the folder is not shown).
	pub file_name: String,
	/// "bank:program name" of the F5 preset (all channels except 10).
	pub preset_label: String,
}

impl View {
	pub fn from_session(session: &Session) -> Self {
		let audio = session.audio().map(|stream| AudioView {
			device: stream.device_name.clone(),
			sample_rate: stream.sample_rate,
			buffer: match stream.buffer_size {
				BufferSize::Fixed(frames) => Some(frames),
				BufferSize::Default => None,
			},
		});
		let bank = session.bank.as_ref().map(|loaded| BankView {
			file_name: loaded
				.path
				.file_name()
				.map_or_else(|| loaded.path.display().to_string(), |name| name.to_string_lossy().into_owned()),
			preset_label: session.preset_info().map(preset_label).unwrap_or_default(),
		});
		Self {
			midi: session.midi_status.clone(),
			audio,
			bank,
			reverb: session.reverb_level(),
			chorus: session.chorus_level(),
			record_dir: session.record_dir().display().to_string(),
			record_format: session.config.record.format,
			xruns: session.shared.xruns(),
			dropped: session.midi_dropped.load(std::sync::atomic::Ordering::Relaxed),
			transpose: session.transpose,
			omni_channel: session.omni_channel(),
			record_dropped: session.shared.record_dropped(),
			held_notes: session.shared.held_notes(),
			max_voices: session.shared.max_voices(),
			load_percent: session.shared.load_percent(),
			levels: session.shared.levels(),
			mixer: session.mixer.clone(),
			notice: session.notice.clone(),
		}
	}
}

/// Which line of the screen a notice belongs to (spec v1.1 §2, "Состояния строк").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoticeTarget {
	Midi,
	Audio,
	Sf2,
	Record,
	/// The yellow message line under the settings.
	Message,
}

pub(super) fn notice_target(notice: &Notice) -> NoticeTarget {
	match notice {
		Notice::MidiError(_) => NoticeTarget::Midi,
		Notice::NoAudioDevice(_) | Notice::AudioError(_) => NoticeTarget::Audio,
		Notice::Sf2Error(_) => NoticeTarget::Sf2,
		Notice::RecordError(_) => NoticeTarget::Record,
		Notice::AudioFallback
		| Notice::AudioReopened(_)
		| Notice::MidiFallback
		| Notice::ConfigError(_)
		| Notice::RecordSaved(_)
		| Notice::FontFallback(_) => NoticeTarget::Message,
	}
}

/// The raw error text carried by a line-bound notice.
fn notice_error(notice: &Notice) -> Option<&str> {
	match notice {
		Notice::MidiError(error)
		| Notice::NoAudioDevice(error)
		| Notice::AudioError(error)
		| Notice::Sf2Error(error)
		| Notice::RecordError(error) => Some(error),
		_ => None,
	}
}

impl View {
	/// Error text for the line `target`, if the current notice belongs there.
	pub(super) fn line_error(&self, target: NoticeTarget) -> Option<&str> {
		let notice = self.notice.as_ref()?;
		(notice_target(notice) == target).then(|| notice_error(notice)).flatten()
	}
}

pub(super) fn notice_text(notice: &Notice, locale: &str) -> String {
	match notice {
		Notice::NoAudioDevice(error) => t!("notice.no_audio_device", locale = locale, error = error).into_owned(),
		Notice::AudioFallback => t!("notice.audio_fallback", locale = locale).into_owned(),
		Notice::AudioReopened(device) => t!("notice.audio_reopened", locale = locale, device = device).into_owned(),
		Notice::AudioError(error) => t!("notice.audio_error", locale = locale, error = error).into_owned(),
		Notice::MidiFallback => t!("notice.midi_fallback", locale = locale).into_owned(),
		Notice::MidiError(error) => t!("notice.midi_error", locale = locale, error = error).into_owned(),
		Notice::Sf2Error(error) => t!("notice.sf2_error", locale = locale, error = error).into_owned(),
		Notice::ConfigError(error) => t!("notice.config_error", locale = locale, error = error).into_owned(),
		Notice::RecordSaved(name) => t!("notice.record_saved", locale = locale, name = name).into_owned(),
		Notice::RecordError(error) => t!("notice.record_error", locale = locale, error = error).into_owned(),
		Notice::FontFallback(font) => t!("notice.font_fallback", locale = locale, font = font).into_owned(),
	}
}
