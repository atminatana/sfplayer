//! The main screen of the window (spec v1.2 §3): the signal path top to bottom — MIDI sources, the
//! processor, the output, the audio file — and the status line pinned to the bottom row. Buttons are text
//! in square brackets; an error is shown red in the line of the setting it belongs to.
//!
//! Draws into a `ScreenBuffer` only, so the whole screen is testable without a window. All text goes
//! through `t!` (ADR 0008).

use std::time::Duration;

use gterm::buffer::ScreenBuffer;
use gterm::cell::Color;
use gterm_widgets::widgets::Meter;
use gterm_widgets::{BufferExt, Rect, Style, Widget};
use rust_i18n::t;

use super::app::App;
use super::meter::{dbfs_text, level_ratio};
use super::view::{NoticeTarget, View, notice_target, notice_text};
use crate::config::RecordFormat;
use crate::midi::MidiStatus;

/// The smallest grid the screen fits in (spec v1.2 §3); below it only a hint is drawn.
pub const MIN_COLS: u16 = 96;
pub const MIN_ROWS: u16 = 31;

// Palette of the mock-up: black background, light text, grey labels and rules, amber values and active
// buttons, green for "fine", red for errors.
pub const BACKGROUND: Color = Color::from_rgb(0x00, 0x00, 0x00);
pub const TEXT: Color = Color::from_rgb(0xE5, 0xE5, 0xE5);
pub const LABEL: Color = Color::from_rgb(0x73, 0x73, 0x73);
pub const RULE: Color = Color::from_rgb(0x40, 0x40, 0x40);
pub const HINT: Color = Color::from_rgb(0x60, 0x60, 0x60);
pub const DISABLED: Color = Color::from_rgb(0x40, 0x40, 0x40);
pub const VALUE: Color = Color::from_rgb(0xFC, 0xD3, 0x4D);
pub const GOOD: Color = Color::from_rgb(0x22, 0xC5, 0x5E);
pub const ERROR: Color = Color::from_rgb(0xEF, 0x44, 0x44);
pub const MESSAGE: Color = Color::from_rgb(0xFA, 0xCC, 0x15);
/// Unlit cells of a meter: a shade glyph covers a quarter of the cell, so a rule-dark colour would vanish.
pub const METER_TRACK: Color = LABEL;

/// Columns of a row: the label, an indented sub-label, and where values start (an 18-cell label column).
const LABEL_X: u16 = 1;
const SUB_LABEL_X: u16 = 3;
const VALUE_X: u16 = 19;
/// Column of the octave / velocity read-out in the keyboard row.
const KEYBOARD_STATE_X: u16 = 69;
/// Level meters L / R and the master strip, in cells (spec v1.2 §3).
const METER_CELLS: u16 = 26;
const MASTER_CELLS: u16 = 20;
/// Meter colour zones (spec v1.2 §3): green up to 78 %, amber up to 92 %, red above.
const METER_ZONES: [(f64, Style); 3] = [
	(0.0, Style::new().fg(GOOD)),
	(0.78, Style::new().fg(VALUE)),
	(0.92, Style::new().fg(ERROR)),
];
/// The on-screen piano: the text keyboard's range, C to E of the next octave (10 white keys, 7 black).
const PIANO_TOP: &str = "│ ██ ██  │ ██ ██ ██  │ ██ ██  │";
const PIANO_BOTTOM: &str = "│  │  │  │  │  │  │  │  │  │  │";
const PIANO_CELLS: u16 = 31;
const RULE_GLYPH: char = '─';
const NOTHING: &str = "—";
const MINUS: &str = "−";
const PLUS: &str = "+";
const RECORD_DOT: &str = "●";
/// Half of the blink period of the recording dot (spec v1.1 §4.3: period 1 s).
const RECORD_BLINK_HALF_PERIOD_MS: u128 = 500;

/// How a `[button]` looks (spec v1.2 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
	Normal,
	/// The selected one of several: inverted on amber.
	Active,
	/// Not available now: dark, does not react.
	Disabled,
}

/// Writes consecutive pieces of one row, advancing to the right.
struct Pen<'a> {
	buf: &'a mut ScreenBuffer,
	x: u16,
	y: u16,
}

impl<'a> Pen<'a> {
	fn at(buf: &'a mut ScreenBuffer, x: u16, y: u16) -> Self {
		Self { buf, x, y }
	}

	fn text(&mut self, text: &str, color: Color) -> &mut Self {
		self.styled(text, Style::new().fg(color))
	}

	fn styled(&mut self, text: &str, style: Style) -> &mut Self {
		let clip = self.buf.area();
		self.buf.set_string(self.x, self.y, text, style, clip);
		let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
		self.x = self.x.saturating_add(width);
		self
	}

	fn gap(&mut self, cells: u16) -> &mut Self {
		self.x = self.x.saturating_add(cells);
		self
	}

	fn button(&mut self, label: &str, look: Look) -> &mut Self {
		let style = match look {
			Look::Normal => Style::new().fg(TEXT),
			Look::Active => Style::new().fg(BACKGROUND).bg(VALUE).bold(),
			Look::Disabled => Style::new().fg(DISABLED),
		};
		self.styled(&format!("[{label}]"), style)
	}
}

pub fn render(buf: &mut ScreenBuffer, app: &App, view: &View) {
	let locale = app.locale;
	buf.clear_all();
	let area = buf.area();
	if area.width < MIN_COLS || area.height < MIN_ROWS {
		let hint = t!("notice.window_too_small", locale = locale, width = MIN_COLS, height = MIN_ROWS);
		Pen::at(buf, 0, 0).text(&hint, ERROR);
		return;
	}

	title(buf, locale);
	sources(buf, 2, app, locale, view);
	handler(buf, 10, locale, view);
	output(buf, 15, app, locale, view);
	audio_file(buf, 21, app, locale, view);
	status(buf, area.height - 1, locale, view);
}

/// A horizontal rule across the row from `x` to the right edge.
fn rule(buf: &mut ScreenBuffer, x: u16, y: u16) {
	let width = buf.area().width.saturating_sub(x);
	buf.fill(Rect::new(x, y, width, 1), RULE_GLYPH, Style::new().fg(RULE));
}

/// `──── sf2_player 0.1.0 ────`: the name and version centred in a rule.
fn title(buf: &mut ScreenBuffer, locale: &str) {
	rule(buf, 0, 0);
	let text = format!(" {} ", t!("app.title", locale = locale, version = env!("CARGO_PKG_VERSION")));
	let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
	let x = buf.area().width.saturating_sub(width) / 2;
	Pen::at(buf, x, 0).text(&text, LABEL);
}

/// `TITLE ─────`; returns the pen after the title so the caller can add buttons before the rule.
fn section(buf: &mut ScreenBuffer, y: u16, title: &str) -> u16 {
	let mut pen = Pen::at(buf, 0, y);
	pen.text(title, LABEL).gap(1);
	pen.x
}

fn label(buf: &mut ScreenBuffer, y: u16, text: &str) {
	Pen::at(buf, LABEL_X, y).text(text, LABEL);
}

fn sub_label(buf: &mut ScreenBuffer, y: u16, text: &str) {
	Pen::at(buf, SUB_LABEL_X, y).text(text, LABEL);
}

fn sources(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View) {
	let after_title = section(buf, y, &t!("main.section_sources", locale = locale));
	rule(buf, after_title, y);

	label(buf, y + 1, &t!("main.keyboard", locale = locale));
	Pen::at(buf, VALUE_X, y + 1).text(NOTHING, DISABLED);
	let keyboard_state = t!(
		"status.octave",
		locale = locale,
		octave = app.keyboard.octave(),
		velocity = format!("{:>3}", app.keyboard.velocity())
	);
	Pen::at(buf, KEYBOARD_STATE_X, y + 1).text(&keyboard_state, HINT);
	let piano_x = buf.area().width.saturating_sub(PIANO_CELLS) / 2;
	Pen::at(buf, piano_x, y + 2).text(PIANO_TOP, HINT);
	Pen::at(buf, piano_x, y + 3).text(PIANO_BOTTOM, HINT);

	let midi_in = t!("main.midi_in", locale = locale);
	label(buf, y + 4, &midi_in);
	let mut pen = Pen::at(buf, VALUE_X, y + 4);
	pen.button(&midi_in, Look::Normal).gap(2);
	match &view.midi {
		MidiStatus::Connected(port) => {
			pen.text(port, TEXT).gap(2).text(&t!("main.opened", locale = locale), GOOD);
		}
		MidiStatus::Disconnected(port) => {
			pen.text(&t!("header.disconnected", locale = locale, port = port), ERROR);
		}
		MidiStatus::NoPort => {
			pen.text(&t!("header.no_device", locale = locale), ERROR);
		}
	}
	if let Some(error) = view.line_error(NoticeTarget::Midi) {
		pen.gap(2).text(error, ERROR);
	}
	sub_label(buf, y + 5, &t!("main.notes", locale = locale));
	Pen::at(buf, VALUE_X, y + 5).text(NOTHING, DISABLED);

	// The MIDI file source arrives with plan stage 5; until then its controls are shown unavailable.
	let midi_file = t!("main.midi_file", locale = locale);
	label(buf, y + 6, &midi_file);
	Pen::at(buf, VALUE_X, y + 6)
		.button(&midi_file, Look::Disabled)
		.gap(2)
		.text(&t!("main.no_file", locale = locale), HINT)
		.gap(2)
		.button(&t!("main.start", locale = locale), Look::Disabled)
		.gap(1)
		.button(&t!("main.pause", locale = locale), Look::Disabled)
		.gap(1)
		.button(&t!("main.stop", locale = locale), Look::Disabled);
}

fn handler(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View) {
	let after_title = section(buf, y, &t!("main.section_handler", locale = locale));
	// The built-in synth arrives with plan stage 4; until then Sound Font is the only processor.
	let mut pen = Pen::at(buf, after_title + 1, y);
	pen.button(&t!("main.engine_synth", locale = locale), Look::Disabled)
		.gap(1)
		.button(&t!("main.engine_sf2", locale = locale), Look::Active)
		.gap(1);
	let after_buttons = pen.x;
	rule(buf, after_buttons, y);

	let sf2_file = t!("main.sf2_file", locale = locale);
	label(buf, y + 1, &sf2_file);
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	pen.button(&sf2_file, Look::Normal).gap(2);
	let sf2_error = view.line_error(NoticeTarget::Sf2);
	match (&view.bank, sf2_error) {
		(Some(bank), None) => {
			pen.text(&bank.file_name, TEXT)
				.gap(2)
				.text(&t!("header.audio_ok", locale = locale), GOOD);
		}
		(Some(bank), Some(error)) => {
			pen.text(&bank.file_name, TEXT).gap(2).text(error, ERROR);
		}
		(None, Some(error)) => {
			pen.text(error, ERROR);
		}
		(None, None) => {
			pen.text(&t!("header.sf2_missing", locale = locale), ERROR);
		}
	}

	let preset = t!("main.preset", locale = locale);
	label(buf, y + 2, &preset);
	let mut pen = Pen::at(buf, VALUE_X, y + 2);
	match &view.bank {
		Some(bank) => {
			pen.button(&preset, Look::Normal).gap(2).text(&bank.preset_label, TEXT);
		}
		None => {
			pen.button(&preset, Look::Disabled).gap(2).text(NOTHING, DISABLED);
		}
	}

	let effects = t!("main.effects", locale = locale);
	label(buf, y + 3, &effects);
	let off = t!("mixer.off", locale = locale);
	let level = |value: Option<u8>| value.map_or_else(|| off.to_string(), |level| level.to_string());
	let sends = format!(
		"{} {}  {} {}",
		t!("mixer.reverb", locale = locale),
		level(view.reverb),
		t!("mixer.chorus", locale = locale),
		level(view.chorus)
	);
	Pen::at(buf, VALUE_X, y + 3)
		.button(&effects, Look::Normal)
		.gap(2)
		.text(&sends, TEXT);
}

fn output(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View) {
	let after_title = section(buf, y, &t!("main.section_output", locale = locale));
	rule(buf, after_title, y);

	let audio_out = t!("main.audio_out", locale = locale);
	label(buf, y + 1, &audio_out);
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	pen.button(&audio_out, Look::Normal).gap(2);
	let audio_error = view.line_error(NoticeTarget::Audio);
	match &view.audio {
		Some(audio) => {
			let buffer = match audio.buffer {
				Some(frames) => t!("header.buffer", locale = locale, frames = frames),
				None => t!("header.buffer_default", locale = locale),
			};
			pen.text(&audio.device, TEXT)
				.gap(2)
				.text(&format!("{} Hz / {buffer}", audio.sample_rate), HINT)
				.gap(2);
			match audio_error {
				Some(error) => pen.text(error, ERROR),
				None => pen.text(&t!("header.audio_ok", locale = locale), GOOD),
			};
		}
		None => {
			let no_device = t!("header.no_device", locale = locale);
			pen.text(audio_error.unwrap_or(&no_device), ERROR);
		}
	}

	let silence = t!("meter.silence", locale = locale);
	for (row, name, peak, rms) in [
		(y + 2, "L", view.levels.peak_left, view.levels.rms_left),
		(y + 3, "R", view.levels.peak_right, view.levels.rms_right),
	] {
		label(buf, row, name);
		level_meter(buf, VALUE_X, row, METER_CELLS, level_ratio(rms));
		let mut pen = Pen::at(buf, VALUE_X + METER_CELLS + 2, row);
		pen.text(&dbfs_text(peak, &silence), TEXT);
		if app.clipping {
			pen.gap(2).styled(&t!("meter.clip", locale = locale), Style::new().fg(ERROR).bold());
		}
	}

	label(buf, y + 4, &t!("main.master", locale = locale));
	let mixer = &view.mixer;
	let mut pen = Pen::at(buf, VALUE_X, y + 4);
	pen.button(MINUS, Look::Normal).gap(1);
	let meter_x = pen.x;
	level_meter(buf, meter_x, y + 4, MASTER_CELLS, f64::from(mixer.master) / 100.0);
	let mut pen = Pen::at(buf, meter_x + MASTER_CELLS + 1, y + 4);
	pen.text(&format!("{:>3}%", mixer.master), VALUE).gap(1).button(PLUS, Look::Normal);
	if mixer.master_muted {
		pen.gap(2)
			.styled(&t!("mixer.mute", locale = locale), Style::new().fg(MESSAGE).bold());
	}
}

fn level_meter(buf: &mut ScreenBuffer, x: u16, y: u16, cells: u16, ratio: f64) {
	Meter::new()
		.ratio(ratio)
		.zones(&METER_ZONES)
		.unfilled_style(Style::new().fg(METER_TRACK))
		.render(Rect::new(x, y, cells, 1), buf);
}

fn audio_file(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View) {
	let after_title = section(buf, y, &t!("main.section_record", locale = locale));
	rule(buf, after_title, y);

	label(buf, y + 1, &t!("main.folder", locale = locale));
	Pen::at(buf, VALUE_X, y + 1)
		.button(&t!("main.folder_button", locale = locale), Look::Normal)
		.gap(2)
		.text(&view.record_dir, TEXT);

	label(buf, y + 2, &t!("main.format", locale = locale));
	let chosen = |format: RecordFormat| {
		if view.record_format == format { Look::Active } else { Look::Normal }
	};
	// Recording MIDI arrives with plan stage 6; until then that format is shown unavailable.
	Pen::at(buf, VALUE_X, y + 2)
		.button("FLAC", chosen(RecordFormat::Flac))
		.gap(1)
		.button("OGG", chosen(RecordFormat::OggVorbis))
		.gap(1)
		.button("MIDI", Look::Disabled)
		.gap(2)
		.text(&t!("main.format_audio", locale = locale), HINT);

	label(buf, y + 3, &t!("main.record", locale = locale));
	let recording = app.recording;
	let unless = |unavailable: bool| if unavailable { Look::Disabled } else { Look::Normal };
	let mut pen = Pen::at(buf, VALUE_X, y + 3);
	pen.button(&t!("main.start", locale = locale), unless(recording.is_some()))
		.gap(1)
		.button(&t!("main.stop", locale = locale), unless(recording.is_none()))
		.gap(1)
		// Cancelling a take arrives with plan stage 6.
		.button(&t!("main.cancel", locale = locale), Look::Disabled)
		.gap(2);
	match (recording, view.line_error(NoticeTarget::Record)) {
		(Some(elapsed), _) => {
			let time = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);
			let indicator = format!("{} {}", record_dot(elapsed), t!("status.rec", locale = locale, time = time));
			pen.styled(&indicator, Style::new().fg(ERROR).bold());
		}
		(None, Some(error)) => {
			pen.text(error, ERROR);
		}
		(None, None) => {
			pen.text(&t!("main.no_takes", locale = locale), HINT);
		}
	}

	if let Some(notice) = view.notice.as_ref().filter(|notice| notice_target(notice) == NoticeTarget::Message) {
		Pen::at(buf, LABEL_X, y + 4).text(&notice_text(notice, locale), MESSAGE);
	}
}

/// The recording dot, lit on even half-periods of the take's elapsed time; a space keeps the width constant.
fn record_dot(elapsed: Duration) -> &'static str {
	if (elapsed.as_millis() / RECORD_BLINK_HALF_PERIOD_MS).is_multiple_of(2) {
		RECORD_DOT
	} else {
		" "
	}
}

/// Bottom row (spec v1.2 §3): counters of constant width on the left, then the optional fields that are
/// active, and the processor summary on the right when it fits.
fn status(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View) {
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
	let mut fields = vec![
		t!("status.voices", locale = locale, voices = voices).into_owned(),
		t!(
			"status.cpu",
			locale = locale,
			percent = format!("{:>3}", view.load_percent.min(100))
		)
		.into_owned(),
		t!("status.xruns", locale = locale, count = xruns).into_owned(),
	];
	if view.transpose != 0 {
		fields.push(t!("status.transpose", locale = locale, value = format!("{:+}", view.transpose)).into_owned());
	}
	if let Some(channel) = view.omni_channel {
		fields.push(t!("status.omni", locale = locale, channel = channel + 1).into_owned());
	}
	if view.dropped > 0 {
		fields.push(t!("status.dropped", locale = locale, count = view.dropped).into_owned());
	}
	if view.record_dropped > 0 {
		fields.push(t!("status.rec_dropped", locale = locale, count = view.record_dropped).into_owned());
	}
	let mut pen = Pen::at(buf, LABEL_X, y);
	pen.text(&fields.join("  "), LABEL);
	let left_end = pen.x;

	let bank = view.bank.as_ref().map_or(NOTHING, |bank| bank.file_name.as_str());
	let engine = t!("status.engine", locale = locale, name = format!("SF2 · {bank}"));
	let width = u16::try_from(engine.chars().count()).unwrap_or(u16::MAX);
	let x = buf.area().width.saturating_sub(width.saturating_add(1));
	// The counters always win: the summary is drawn only when it does not touch them.
	if x > left_end + 1 {
		Pen::at(buf, x, y).text(&engine, VALUE);
	}
}

#[cfg(test)]
mod tests;
