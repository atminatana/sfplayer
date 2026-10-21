//! Drawing the main screen (spec v1.1 §2, ADR 0010): a settings list keyed by F-keys, the mixer, and the
//! status counters in the bottom border. All text goes through `t!` (ADR 0008). An error is shown red in
//! the line of the setting it belongs to; notices not tied to a line go to the message line in yellow.

use cpal::BufferSize;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use rust_i18n::t;

use super::app::App;
use super::app::Modal;
use super::dialog::preset_label;
use super::glyphs::Glyphs;
use super::meter::{dbfs_text, level_bar};
use super::modals;
use super::text_keyboard::NoteOffMode;
use crate::audio::{Levels, Mixer};
use crate::config::RecordFormat;
use crate::midi::MidiStatus;
use crate::session::{Notice, Session};

/// Spec §6: the layout is designed for at least 80×24.
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;
/// Rows of the settings block: `[F1]`…`[F7]`, the Home/End line, the message line and a blank separator.
const SETTINGS_ROWS: u16 = 10;
/// Width of the level bars in the mixer.
const METER_BAR_WIDTH: usize = 12;
/// The keyboard-mode token of the status line is padded to this many characters so later fields never move.
const KEYBOARD_MODE_WIDTH: usize = 16;

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
	pub buffer: BufferSize,
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
			buffer: stream.buffer_size,
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
enum NoticeTarget {
	Midi,
	Audio,
	Sf2,
	Record,
	/// The yellow message line under the settings.
	Message,
}

fn notice_target(notice: &Notice) -> NoticeTarget {
	match notice {
		Notice::MidiError(_) => NoticeTarget::Midi,
		Notice::NoAudioDevice(_) | Notice::AudioError(_) => NoticeTarget::Audio,
		Notice::Sf2Error(_) => NoticeTarget::Sf2,
		Notice::RecordError(_) => NoticeTarget::Record,
		Notice::AudioFallback | Notice::AudioReopened(_) | Notice::MidiFallback | Notice::ConfigError(_) | Notice::RecordSaved(_) => {
			NoticeTarget::Message
		}
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
	fn line_error(&self, target: NoticeTarget) -> Option<&str> {
		let notice = self.notice.as_ref()?;
		(notice_target(notice) == target).then(|| notice_error(notice)).flatten()
	}
}

pub fn render(frame: &mut Frame, app: &App, view: &View) {
	let area = frame.area();
	let locale = app.locale;
	if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
		let message = t!("notice.terminal_too_small", locale = locale, width = MIN_WIDTH, height = MIN_HEIGHT);
		frame.render_widget(Paragraph::new(message.as_ref()).style(Style::default().fg(Color::Red)), area);
		return;
	}

	let title = padded(&t!("app.title", locale = locale, version = env!("CARGO_PKG_VERSION")));
	let (fixed, optional) = status_lines(app, locale, view);
	let mut outer = Block::default()
		.borders(Borders::ALL)
		.title_top(Line::from(title).alignment(Alignment::Center));
	// The fixed fields always win: optional ones are drawn only when both fit side by side (80 columns is tight).
	let room = usize::from(area.width.saturating_sub(2));
	if let Some(optional) = optional.filter(|optional| fixed.width() + optional.width() <= room) {
		outer = outer.title_bottom(optional.alignment(Alignment::Right));
	}
	let outer = outer.title_bottom(fixed.alignment(Alignment::Left));
	let inner = outer.inner(area);
	frame.render_widget(outer, area);

	let rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(SETTINGS_ROWS), Constraint::Min(4)])
		.split(inner);
	render_settings(frame, rows[0], app, locale, view);
	render_mixer(frame, rows[1], app, locale, view);
	match &app.modal {
		Some(Modal::Select(dialog)) => modals::render_dialog(frame, area, dialog, &app.glyphs, locale),
		Some(Modal::Sf2Path(input)) => modals::render_path_dialog(frame, area, input, locale),
		Some(Modal::Effects(dialog)) => modals::render_effects_dialog(frame, area, dialog, locale),
		Some(Modal::Record(dialog)) => modals::render_record_dialog(frame, area, dialog, &app.glyphs, locale),
		Some(Modal::Help(scroll)) => modals::render_help(frame, area, &app.bindings, *scroll, locale),
		Some(Modal::ConfirmQuit) => modals::render_confirm_quit(frame, area, locale),
		None => {}
	}
}

fn red(text: impl Into<String>) -> Span<'static> {
	Span::styled(text.into(), Style::default().fg(Color::Red))
}

/// `[F2] MIDI:    value` — the label column is as wide as the widest label of the locale.
fn render_settings(frame: &mut Frame, area: Rect, app: &App, locale: &str, view: &View) {
	let labels = [
		"header.midi",
		"header.audio",
		"header.sf2",
		"header.preset",
		"header.effects",
		"header.record",
	]
	.map(|key| t!(key, locale = locale).into_owned());
	let width = labels.iter().map(|label| label.chars().count()).max().unwrap_or(0);
	let row = |key: u8, label: &str, value: Vec<Span<'static>>| {
		let mut spans = vec![label_span(format!("[F{key}] {label:<width$} "))];
		spans.extend(value);
		Line::from(spans)
	};
	let no_device = t!("header.no_device", locale = locale).into_owned();

	let midi_value = {
		let mut spans = match &view.midi {
			MidiStatus::Connected(port) => vec![Span::raw(port.clone())],
			MidiStatus::Disconnected(port) => vec![red(t!("header.disconnected", locale = locale, port = port).into_owned())],
			MidiStatus::NoPort => vec![red(no_device.clone())],
		};
		if let Some(error) = view.line_error(NoticeTarget::Midi) {
			spans.push(red(format!("  {error}")));
		}
		spans
	};
	let audio_value = match &view.audio {
		Some(audio) => {
			let buffer = match audio.buffer {
				BufferSize::Fixed(frames) => t!("header.buffer", locale = locale, frames = frames).into_owned(),
				BufferSize::Default => t!("header.buffer_default", locale = locale).into_owned(),
			};
			let mut spans = vec![Span::raw(format!("{} {} Hz / {buffer}", audio.device, audio.sample_rate))];
			match view.line_error(NoticeTarget::Audio) {
				Some(error) => spans.push(red(format!("  {error}"))),
				None => spans.push(Span::styled(
					format!("  {}", t!("header.audio_ok", locale = locale)),
					Style::default().fg(Color::Green),
				)),
			}
			spans
		}
		None => vec![red(view.line_error(NoticeTarget::Audio).map_or(no_device.clone(), str::to_owned))],
	};
	let sf2_value = match &view.bank {
		Some(bank) => {
			let mut spans = vec![Span::raw(bank.file_name.clone())];
			if let Some(error) = view.line_error(NoticeTarget::Sf2) {
				spans.push(red(format!("  {error}")));
			}
			spans
		}
		None => vec![red(view
			.line_error(NoticeTarget::Sf2)
			.map_or_else(|| t!("header.sf2_missing", locale = locale).into_owned(), str::to_owned))],
	};
	let preset_value = match &view.bank {
		Some(bank) => vec![Span::raw(bank.preset_label.clone())],
		None => vec![Span::styled(
			t!("header.no_bank", locale = locale).into_owned(),
			Style::default().fg(Color::DarkGray),
		)],
	};
	let record_value = {
		let mut spans = vec![Span::raw(format!("{}  {}", view.record_dir, format_name(view.record_format)))];
		if let Some(error) = view.line_error(NoticeTarget::Record) {
			spans.push(red(format!("  {error}")));
		}
		if let Some(elapsed) = app.recording {
			spans.push(Span::styled(
				format!(
					"  {} {}",
					record_blink(elapsed, &app.glyphs),
					t!(
						"status.rec",
						locale = locale,
						time = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
					)
				),
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
			));
		}
		spans
	};
	let message = view
		.notice
		.as_ref()
		.filter(|notice| notice_target(notice) == NoticeTarget::Message)
		.map(|notice| Span::styled(notice_text(notice, locale), Style::default().fg(Color::Yellow)))
		.unwrap_or_default();

	let lines = vec![
		Line::from(label_span(format!("[F1] {}", t!("header.help", locale = locale)))),
		row(2, &labels[0], midi_value),
		row(3, &labels[1], audio_value),
		row(4, &labels[2], sf2_value),
		row(5, &labels[3], preset_value),
		row(6, &labels[4], effects_spans(view, locale)),
		row(7, &labels[5], record_value),
		Line::from(Span::styled(
			format!("     {}", t!("header.record_keys", locale = locale)),
			Style::default().fg(Color::DarkGray),
		)),
		Line::from(message),
	];
	frame.render_widget(Paragraph::new(lines), area);
}

/// "rev 40  cho off" — reverb and chorus send levels or "off" (F-13).
fn effects_spans(view: &View, locale: &str) -> Vec<Span<'static>> {
	let off = t!("mixer.off", locale = locale);
	let level = |value: Option<u8>| value.map_or_else(|| off.to_string(), |level| level.to_string());
	vec![Span::raw(format!(
		"{} {}  {} {}",
		t!("mixer.reverb", locale = locale),
		level(view.reverb),
		t!("mixer.chorus", locale = locale),
		level(view.chorus)
	))]
}

/// Format names are proper nouns, the same in every language.
pub(super) fn format_name(format: RecordFormat) -> &'static str {
	match format {
		RecordFormat::Flac => "FLAC",
		RecordFormat::OggVorbis => "Ogg Vorbis",
	}
}

/// Mixer block: title, L/R meters (F-09) and the master strip (F-10, master part).
fn render_mixer(frame: &mut Frame, area: Rect, app: &App, locale: &str, view: &View) {
	let glyphs = &app.glyphs;
	let silence = t!("meter.silence", locale = locale);
	let clip = if app.clipping {
		format!("  {}", t!("meter.clip", locale = locale))
	} else {
		String::new()
	};
	let meter_line = |name: &str, peak: f32, rms: f32| {
		let mut spans = vec![
			Span::raw(format!("  {name}  {}", glyphs.bar_open)),
			Span::raw(level_bar(rms, METER_BAR_WIDTH, glyphs)),
			Span::raw(format!("{}  {}", glyphs.bar_close, dbfs_text(peak, &silence))),
		];
		if !clip.is_empty() {
			spans.push(Span::styled(
				clip.clone(),
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
			));
		}
		Line::from(spans)
	};
	let lines = vec![
		Line::from(label_span(t!("panel.mixer", locale = locale).into_owned())),
		meter_line("L", view.levels.peak_left, view.levels.rms_left),
		meter_line("R", view.levels.peak_right, view.levels.rms_right),
		master_line(view, locale, glyphs),
	];
	frame.render_widget(Paragraph::new(lines), area);
}

/// "  MASTER ████████░░   80%" plus a mute mark.
fn master_line(view: &View, locale: &str, glyphs: &Glyphs) -> Line<'static> {
	let mixer = &view.mixer;
	let filled = ((f32::from(mixer.master) / 100.0) * 10.0).round() as usize;
	let mut spans = vec![
		Span::raw("  "),
		label_span(t!("mixer.master", locale = locale).into_owned()),
		Span::raw(format!(" {}  {:>3}%", glyphs.bar(filled, 10), mixer.master)),
	];
	if mixer.master_muted {
		spans.push(Span::styled(
			format!("  {}", t!("mixer.mute", locale = locale)),
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
		));
	}
	Line::from(spans)
}

/// Bottom-border status (spec v1.1 §4.4): the mandatory fields (constant width, left) and the optional ones
/// (transposition, omni channel, dropped counters; right side, only when active).
fn status_lines(app: &App, locale: &str, view: &View) -> (Line<'static>, Option<Line<'static>>) {
	let voices = if view.max_voices == 0 {
		format!("{:>3}/{:<3}", "-", "-")
	} else {
		format!("{:>3}/{:<3}", view.held_notes.min(999), view.max_voices.min(999))
	};
	let xruns = if view.xruns > 99_999 {
		"9999+".to_owned()
	} else {
		format!("{:>5}", view.xruns)
	};
	let mode = match app.keyboard.mode() {
		NoteOffMode::Release => t!("status.kbd_release", locale = locale).into_owned(),
		NoteOffMode::Gate(gate) => t!("status.kbd_gate", locale = locale, ms = gate.as_millis()).into_owned(),
	};
	let fixed = [
		t!("status.voices", locale = locale, voices = voices).into_owned(),
		t!(
			"status.cpu",
			locale = locale,
			percent = format!("{:>3}", view.load_percent.min(100))
		)
		.into_owned(),
		t!("status.xruns", locale = locale, count = xruns).into_owned(),
		format!("{mode:<KEYBOARD_MODE_WIDTH$}"),
		t!(
			"status.octave",
			locale = locale,
			octave = app.keyboard.octave(),
			velocity = format!("{:>3}", app.keyboard.velocity())
		)
		.into_owned(),
	];
	let mut optional = Vec::new();
	if view.transpose != 0 {
		optional.push(t!("status.transpose", locale = locale, value = format!("{:+}", view.transpose)).into_owned());
	}
	if let Some(channel) = view.omni_channel {
		optional.push(t!("status.omni", locale = locale, channel = channel + 1).into_owned());
	}
	if view.dropped > 0 {
		optional.push(t!("status.dropped", locale = locale, count = view.dropped).into_owned());
	}
	if view.record_dropped > 0 {
		optional.push(t!("status.rec_dropped", locale = locale, count = view.record_dropped).into_owned());
	}
	let optional = (!optional.is_empty()).then(|| Line::from(padded(&optional.join(" "))));
	(Line::from(padded(&fixed.join(" "))), optional)
}

/// Half of the blink period of the recording dot (spec v1.1 §4.3: period 1 s).
const RECORD_BLINK_HALF_PERIOD_MS: u128 = 500;

/// The recording dot, lit on even half-periods of the take's elapsed time; a space keeps the width constant.
pub(super) fn record_blink(elapsed: std::time::Duration, glyphs: &Glyphs) -> &'static str {
	if (elapsed.as_millis() / RECORD_BLINK_HALF_PERIOD_MS).is_multiple_of(2) {
		glyphs.dot
	} else {
		" "
	}
}

fn notice_text(notice: &Notice, locale: &str) -> String {
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
	}
}

pub(super) fn label<'a>(text: impl Into<std::borrow::Cow<'a, str>>) -> Span<'a> {
	Span::styled(text.into(), Style::default().add_modifier(Modifier::BOLD))
}

fn label_span(text: String) -> Span<'static> {
	label(text)
}

/// Spec §6 mock-up: titles sit inside the border with a space on each side.
pub(super) fn padded(title: &str) -> String {
	format!(" {title} ")
}

#[cfg(test)]
mod tests;
