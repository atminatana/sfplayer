//! The main screen of the window (spec v1.2 §3): the signal path top to bottom — MIDI sources, the
//! processor, the output, the audio file — and the status line pinned to the bottom row. Buttons are text
//! in square brackets and the two devices are chosen from dropdowns: the screen says where each control
//! goes, `Controls` draws them and takes their input.
//! An error is shown red in the line of the setting it belongs to.
//!
//! The layout, the helpers shared by the sections and the status line are here; the sections are drawn
//! by the submodules `sources`, `handler` and `output`.
//!
//! Draws into a `ScreenBuffer` only, so the whole screen is testable without a window. All text goes
//! through `t!` (ADR 0008).

mod handler;
mod output;
mod sources;

use gterm::buffer::ScreenBuffer;
use gterm_widgets::{BufferExt, Rect, Style};
use rust_i18n::t;

use self::handler::handler;
use self::output::{audio_file, output};
use self::sources::sources;
use super::app::{App, ButtonId};
use super::controls::{Controls, Look};
use super::paint::Pen;
pub use super::paint::{BACKGROUND, DISABLED, ERROR, GOOD, HINT, LABEL, MESSAGE, METER_TRACK, RULE, TEXT, VALUE};
use super::piano;
use super::view::View;
use crate::config::EngineKind;
use crate::synth::Wave;

/// The smallest grid the screen fits in (spec v1.2 §3); below it only a hint is drawn.
pub const MIN_COLS: u16 = 96;
pub const MIN_ROWS: u16 = 31;

/// Columns of a row: the label, an indented sub-label, and where values start (an 18-cell label column).
const LABEL_X: u16 = 1;
const SUB_LABEL_X: u16 = 3;
const VALUE_X: u16 = 19;
/// Cells of the field of a device dropdown: the name of the device and the marker at its end.
const DEVICE_CELLS: u16 = 40;
/// Column of the octave / velocity read-out in the keyboard row.
const KEYBOARD_STATE_X: u16 = 69;
/// The position bar of the MIDI file: its cells and the mark of the part not played yet.
const POSITION_CELLS: u16 = 24;
const POSITION_TRACK: char = '·';
/// Level meters L / R and the master strip, in cells (spec v1.2 §3).
const METER_CELLS: u16 = 26;
const MASTER_CELLS: u16 = 20;
/// Meter colour zones (spec v1.2 §3): green up to 78 %, amber up to 92 %, red above.
const METER_ZONES: [(f64, Style); 3] = [
	(0.0, Style::new().fg(GOOD)),
	(0.78, Style::new().fg(VALUE)),
	(0.92, Style::new().fg(ERROR)),
];
/// Row of the "MIDI sources" section, and of the piano within it.
const SOURCES_Y: u16 = 2;
const PIANO_ROW: u16 = 2;
/// Row of the "Processor" section; the sections under it start lower by the rows the processor
/// has beyond the three of a SoundFont.
const HANDLER_Y: u16 = 10;
const OUTPUT_Y: u16 = 15;
const AUDIO_FILE_Y: u16 = 21;
/// The built-in synthesizer takes five rows where a SoundFont takes three (spec v1.2 §3).
const SYNTH_EXTRA_ROWS: u16 = 2;
/// Where the second value of an envelope row starts, from the value column.
const SECOND_VALUE_X: u16 = VALUE_X + 26;
const RULE_GLYPH: char = '─';
const NOTHING: &str = "—";
const MINUS: &str = "−";
const PLUS: &str = "+";
const RECORD_DOT: &str = "●";
/// Half of the blink period of the recording dot (spec v1.1 §4.3: period 1 s).
const RECORD_BLINK_HALF_PERIOD_MS: u128 = 500;

pub fn render(buf: &mut ScreenBuffer, app: &App, view: &View, controls: &mut Controls) {
	let locale = app.locale;
	buf.clear_all();
	controls.begin_frame();
	let area = buf.area();
	if area.width < MIN_COLS || area.height < MIN_ROWS {
		let hint = t!("notice.window_too_small", locale = locale, width = MIN_COLS, height = MIN_ROWS);
		Pen::at(buf, 0, 0).text(&hint, ERROR);
		// No button is placed, so none reacts while the screen is not shown.
		controls.finish_frame(buf);
		return;
	}

	title(buf, locale);
	sources(buf, SOURCES_Y, app, locale, view, controls);
	let extra = match view.engine {
		EngineKind::Sf2 => 0,
		EngineKind::Builtin => SYNTH_EXTRA_ROWS,
	};
	handler(buf, HANDLER_Y, locale, view, controls);
	output(buf, OUTPUT_Y + extra, app, locale, view, controls);
	audio_file(buf, AUDIO_FILE_Y + extra, app, locale, view, controls);
	status(buf, area.height - 1, locale, view);
	controls.finish_frame(buf);
}

/// The top-left cell of the piano on a grid of `area`: centred, whatever the width (spec v1.2 §3).
/// `None` while the grid is too small for the screen to be shown.
pub fn piano_origin(area: Rect) -> Option<(u16, u16)> {
	let is_shown = area.width >= MIN_COLS && area.height >= MIN_ROWS;
	is_shown.then(|| (area.width.saturating_sub(piano::WIDTH) / 2, SOURCES_Y + PIANO_ROW))
}

/// Gives a button its place at the pen and moves the pen past it; the button is drawn when the frame is finished.
fn put(pen: &mut Pen<'_>, controls: &mut Controls, id: ButtonId, label: &str, look: Look) {
	let width = controls.place(id, pen.x, pen.y, label, look);
	pen.gap(width);
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

/// `text` cut to `room` cells; what is cut off is marked with `…`.
fn fitted(text: &str, room: usize) -> String {
	if text.chars().count() <= room {
		return text.to_owned();
	}
	let kept: String = text.chars().take(room.saturating_sub(1)).collect();
	format!("{kept}…")
}

/// The width of the grid the pen writes into.
fn buf_width(pen: &Pen<'_>) -> u16 {
	pen.clip_width()
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

	let summary = match view.engine {
		EngineKind::Sf2 => {
			let bank = view.bank.as_ref().map_or(NOTHING, |bank| bank.file_name.as_str());
			format!("SF2 · {bank}")
		}
		EngineKind::Builtin => {
			let wave = match view.builtin.wave {
				Wave::Sine => t!("main.wave_sine", locale = locale),
				Wave::Saw => t!("main.wave_saw", locale = locale),
				Wave::Triangle => t!("main.wave_triangle", locale = locale),
			};
			format!("Synth · {wave}")
		}
	};
	let engine = t!("status.engine", locale = locale, name = summary);
	let width = u16::try_from(engine.chars().count()).unwrap_or(u16::MAX);
	let x = buf.area().width.saturating_sub(width.saturating_add(1));
	// The counters always win: the summary is drawn only when it does not touch them.
	if x > left_end + 1 {
		Pen::at(buf, x, y).text(&engine, VALUE);
	}
}

#[cfg(test)]
mod tests;
