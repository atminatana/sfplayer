//! What the screen shows, decoupled from live handles and from the drawing backend, so that rendering is
//! testable (spec v1.1 §2; plan v1.2 step 1.2). Also the texts that depend only on this model: which
//! line a notice belongs to and how it reads.

use cpal::BufferSize;
use rust_i18n::t;

use super::dialog::preset_label;
use crate::audio::{Levels, Mixer, Source};
use crate::config::{BuiltinConfig, EngineKind, RecordFormat};
use crate::midi::MidiStatus;
use crate::midi::player::Transport;
use crate::session::{MidiFile, Notice, Session};

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
	/// Keys sounding now, a bit per MIDI key: played in the window (text keyboard, mouse) and coming
	/// from the MIDI input (F-24).
	pub keyboard_notes: u128,
	pub midi_notes: u128,
	/// The chosen processor and the timbre of the built-in synthesizer (F-22).
	pub engine: EngineKind,
	pub builtin: BuiltinConfig,
	/// The MIDI file and where its playback is (F-21).
	pub file: FileView,
	/// The take being recorded, or the last one saved (F-23).
	pub take: TakeView,
}

/// What the record row says about takes. Sizes are kept as the row shows them, so that a growing
/// file redraws the window only when the shown number changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeView {
	/// Nothing recorded in this session yet.
	None,
	Recording {
		name: String,
		size: Size,
	},
	Saved {
		path: String,
		size: Size,
	},
}

/// The size of a file as the screen shows it: whole kilobytes under a megabyte — a MIDI take is a few
/// of them — and tenths of a megabyte from there on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
	Kilobytes(u64),
	TenthsOfMegabyte(u64),
}

impl Size {
	pub fn of(bytes: u64) -> Self {
		const KILOBYTE: u64 = 1024;
		let kilobytes = (bytes + KILOBYTE / 2) / KILOBYTE;
		if kilobytes < KILOBYTE {
			Self::Kilobytes(kilobytes)
		} else {
			Self::TenthsOfMegabyte(tenths_of_megabyte(bytes))
		}
	}
}

/// Bytes as tenths of a megabyte, rounded to the nearest.
fn tenths_of_megabyte(bytes: u64) -> u64 {
	const TENTH: u64 = 1024 * 1024 / 10;
	(bytes + TENTH / 2) / TENTH
}

/// The MIDI file source as the screen shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileView {
	pub file: MidiFile,
	pub transport: Transport,
	/// Tenths of a second into the song and its whole length: a finer position would redraw the
	/// window on every tick for a bar that moves much more slowly.
	pub position_tenths: u32,
	pub duration_tenths: u32,
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
			keyboard_notes: session.shared.notes(Source::Keyboard),
			midi_notes: session.shared.notes(Source::MidiIn),
			engine: session.config.synth.engine,
			builtin: session.config.synth.builtin.clone(),
			take: match (session.recording_file(), &session.last_take) {
				(Some((name, bytes)), _) => TakeView::Recording {
					name,
					size: Size::of(bytes),
				},
				(None, Some(take)) => TakeView::Saved {
					path: take.path.display().to_string(),
					size: Size::of(take.bytes),
				},
				(None, None) => TakeView::None,
			},
			file: {
				let status = session.file_status();
				let tenths = |time: std::time::Duration| u32::try_from(time.as_millis() / 100).unwrap_or(u32::MAX);
				FileView {
					file: session.midi_file.clone(),
					transport: status.transport,
					position_tenths: tenths(status.position),
					duration_tenths: tenths(status.duration),
				}
			},
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
		| Notice::RecordCancelled
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

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const ELLIPSIS: &str = "…";

/// The name of a MIDI key with its octave: 60 is C4.
pub(super) fn note_name(key: u8) -> String {
	format!("{}{}", NOTE_NAMES[usize::from(key % 12)], i16::from(key / 12) - 1)
}

/// The names of the keys set in `keys`, lowest first, separated by spaces: `C4 E4 G4`. What does
/// not fit into `width` cells is cut at a whole name and marked with `…`. `None` when no key is set.
pub(super) fn note_names(keys: u128, width: usize) -> Option<String> {
	let names: Vec<String> = (0..128u8).filter(|key| keys & (1u128 << key) != 0).map(note_name).collect();
	if names.is_empty() {
		return None;
	}
	let all = names.join(" ");
	if all.chars().count() <= width {
		return Some(all);
	}
	let mut shown = String::new();
	for name in &names {
		// Room is kept for the space after the name and for the mark.
		if shown.chars().count() + name.chars().count() + 2 > width {
			break;
		}
		shown.push_str(name);
		shown.push(' ');
	}
	shown.push_str(ELLIPSIS);
	Some(shown)
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
		Notice::RecordCancelled => t!("notice.record_cancelled", locale = locale).into_owned(),
		Notice::RecordError(error) => t!("notice.record_error", locale = locale, error = error).into_owned(),
		Notice::FontFallback(font) => t!("notice.font_fallback", locale = locale, font = font).into_owned(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn keys(sounding: &[u8]) -> u128 {
		sounding.iter().fold(0, |map, key| map | (1u128 << key))
	}

	#[test]
	fn small_files_are_sized_in_kilobytes_and_large_ones_in_tenths_of_a_megabyte() {
		assert_eq!(Size::of(0), Size::Kilobytes(0));
		assert_eq!(Size::of(511), Size::Kilobytes(0));
		assert_eq!(Size::of(512), Size::Kilobytes(1), "half a kilobyte rounds up");
		assert_eq!(
			Size::of(1023 * 1024 + 511),
			Size::Kilobytes(1023),
			"the last size shown in kilobytes"
		);
		assert_eq!(
			Size::of(1023 * 1024 + 512),
			Size::TenthsOfMegabyte(10),
			"what would read 1024 KB is a megabyte"
		);
		assert_eq!(Size::of(5 * 1024 * 1024), Size::TenthsOfMegabyte(50));
	}

	#[test]
	fn sizes_are_rounded_to_the_tenth_of_a_megabyte_the_screen_shows() {
		const MEGABYTE: u64 = 1024 * 1024;
		assert_eq!(tenths_of_megabyte(0), 0);
		assert_eq!(tenths_of_megabyte(MEGABYTE / 20 - 1), 0, "just under 0.05 MB");
		assert_eq!(tenths_of_megabyte(MEGABYTE / 20 + 1), 1, "just over 0.05 MB");
		assert_eq!(tenths_of_megabyte(MEGABYTE), 10);
		assert_eq!(tenths_of_megabyte(MEGABYTE * 5 / 4 + 1), 13, "1.25 MB and a byte");
		assert_eq!(tenths_of_megabyte(MEGABYTE * 1024), 10_240, "a gigabyte");
	}

	#[test]
	fn keys_are_named_by_note_and_octave() {
		assert_eq!(note_name(60), "C4");
		assert_eq!(note_name(61), "C#4");
		assert_eq!(note_name(69), "A4");
		assert_eq!(note_name(59), "B3");
		assert_eq!(note_name(0), "C-1");
		assert_eq!(note_name(127), "G9");
	}

	#[test]
	fn note_names_are_listed_lowest_first_or_not_at_all() {
		assert_eq!(note_names(0, 40), None);
		assert_eq!(note_names(keys(&[67, 60, 64]), 40).as_deref(), Some("C4 E4 G4"));
		assert_eq!(note_names(keys(&[0, 127]), 40).as_deref(), Some("C-1 G9"));
	}

	#[test]
	fn names_that_do_not_fit_are_cut_at_a_whole_name_and_marked() {
		let chord = keys(&[60, 64, 67]);
		assert_eq!(note_names(chord, 8).as_deref(), Some("C4 E4 G4"), "exactly fits");
		assert_eq!(note_names(chord, 7).as_deref(), Some("C4 E4 …"));
		assert_eq!(note_names(chord, 6).as_deref(), Some("C4 …"), "E4 would leave no room for the mark");
		assert_eq!(note_names(chord, 4).as_deref(), Some("C4 …"));
		assert_eq!(note_names(chord, 3).as_deref(), Some("…"), "not even one name fits");
		assert_eq!(note_names(chord, 0).as_deref(), Some("…"));
		for width in 1..40 {
			let shown = note_names(keys(&[36, 49, 61, 73, 85, 97, 109]), width).expect("notes sound");
			assert!(shown.chars().count() <= width, "{shown:?} in {width} cells");
		}
	}
}
