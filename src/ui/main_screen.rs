//! The main screen of the window (spec v1.2 §3): a frame around the whole grid with the name and the
//! version in its top border and the status in its bottom one, and inside it the signal path top to
//! bottom — MIDI sources, the engine, the output, the recording. The first two sections hang from the
//! top of the frame, the last two stand on its bottom. Buttons are text in square brackets, and the two
//! devices and the format of takes are chosen from dropdowns: the screen says where each control goes,
//! `Controls` draws them and takes their input.
//! An error is shown red in the line of the setting it belongs to.
//!
//! The layout, the helpers shared by the sections, the frame and the status are here; the sections are
//! drawn by the submodules `sources`, `handler` and `output`.
//!
//! Draws into a `ScreenBuffer` only, so the whole screen is testable without a window. All text goes
//! through `t!` (ADR 0008).

mod handler;
mod output;
mod sources;

use gterm::buffer::ScreenBuffer;
use gterm_widgets::widgets::Block;
use gterm_widgets::{BufferExt, Rect, Style, Widget};
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
pub const MIN_ROWS: u16 = 36;

/// Cells the frame takes on each side of the grid.
const FRAME: u16 = 1;
/// What is inside the frame keeps away from it: two cells at the sides, one row above and below.
const FRAME_INSET_X: u16 = 2;
const FRAME_INSET_Y: u16 = 1;
/// Columns of a row: the label, an indented sub-label, and where values start (an 18-cell label column).
const LABEL_X: u16 = FRAME + FRAME_INSET_X;
const SUB_LABEL_X: u16 = LABEL_X + 2;
const VALUE_X: u16 = LABEL_X + 18;
/// Cells of the field of a device dropdown: the name of the device and the marker at its end.
const DEVICE_CELLS: u16 = 40;
/// Cells of the field of the format dropdown.
const FORMAT_CELLS: u16 = 8;
/// Column of the octave / velocity read-out in the keyboard row.
const KEYBOARD_STATE_X: u16 = VALUE_X + 50;
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
/// A section starts with its rule and a blank row under it; another blank row parts its last row from
/// the rule of the next section.
const SECTION_HEAD_ROWS: u16 = 2;
/// Row of the "MIDI sources" section, and of the piano within it: under the keyboard row.
const SOURCES_Y: u16 = FRAME + FRAME_INSET_Y;
const PIANO_ROW: u16 = SECTION_HEAD_ROWS + 1;
/// Row of the "Engine" section: the sources take their head, the keyboard row, the piano, the MIDI
/// input with its notes, the MIDI file with its position, and a blank row.
const HANDLER_Y: u16 = SOURCES_Y + SECTION_HEAD_ROWS + 1 + piano::HEIGHT + 4 + 1;
/// Rows of the "Output" section down to the rule of "Recording": the head, the device, the two
/// meters, the master and a blank row.
const OUTPUT_ROWS: u16 = SECTION_HEAD_ROWS + 4 + 1;
/// Rows of the "Recording" section down to the inset of the frame: the head, the folder, the format
/// and the take. A message that belongs to no row is written under the take, in the inset row: that
/// row is blank whenever there is nothing to say.
const RECORD_ROWS: u16 = SECTION_HEAD_ROWS + 3;
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

	frame(buf);
	title(buf);
	status(buf, area.height - FRAME, locale, view);
	sources(buf, SOURCES_Y, app, locale, view, controls);
	handler(buf, HANDLER_Y, locale, view, controls);
	// The output and the recording stand on the bottom of the frame, whatever the height of the grid.
	let record_y = area.height - FRAME - FRAME_INSET_Y - RECORD_ROWS;
	output(buf, record_y - OUTPUT_ROWS, app, locale, view, controls);
	audio_file(buf, record_y, app, locale, view, controls);
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

/// The column just past the last one a row of a grid `width` cells wide may take: the inset and the
/// frame are to the right of it.
fn content_end(width: u16) -> u16 {
	width.saturating_sub(FRAME + FRAME_INSET_X)
}

/// A horizontal rule across the row from `x` to the end of the content.
fn rule(buf: &mut ScreenBuffer, x: u16, y: u16) {
	let width = content_end(buf.area().width).saturating_sub(x);
	buf.fill(Rect::new(x, y, width, 1), RULE_GLYPH, Style::new().fg(RULE));
}

/// The single-line frame around the whole grid.
fn frame(buf: &mut ScreenBuffer) {
	let area = buf.area();
	Block::bordered().border_style(Style::new().fg(RULE)).render(area, buf);
}

/// Where a text `width` cells long starts so as to end at the right corner of a border.
fn before_right_corner(buf: &ScreenBuffer, width: usize) -> u16 {
	let width = u16::try_from(width).unwrap_or(u16::MAX);
	buf.area().width.saturating_sub(FRAME.saturating_add(width))
}

/// `┌ sf2_player ──── 0.3.0 ┐`: the name at the left end of the top border and the version at the right
/// one, each a cell away from its corner.
fn title(buf: &mut ScreenBuffer) {
	Pen::at(buf, FRAME, 0).text(concat!(" ", env!("CARGO_PKG_NAME"), " "), LABEL);
	let version = concat!(" ", env!("CARGO_PKG_VERSION"), " ");
	let x = before_right_corner(buf, version.chars().count());
	Pen::at(buf, x, 0).text(version, LABEL);
}

/// `─ TITLE `: the start of the rule of a section. Returns the column after it, where the caller goes
/// on with the rule — or with buttons before it.
fn section(buf: &mut ScreenBuffer, y: u16, title: &str) -> u16 {
	buf.fill(Rect::new(LABEL_X, y, 1, 1), RULE_GLYPH, Style::new().fg(RULE));
	let mut pen = Pen::at(buf, LABEL_X + 2, y);
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

/// The bottom border (spec v1.2 §3): counters of constant width at its left end, then the optional
/// fields that are active, and the engine summary at its right end when it fits. Both keep a cell away
/// from their corner.
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
	let mut pen = Pen::at(buf, FRAME, y);
	pen.text(&format!(" {} ", fields.join("  ")), LABEL);
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
	let engine = format!(" {} ", t!("status.engine", locale = locale, name = summary));
	let x = before_right_corner(buf, engine.chars().count());
	// The counters always win: the summary is drawn only when the border shows between the two.
	if x > left_end {
		Pen::at(buf, x, y).text(&engine, VALUE);
	}
}

#[cfg(test)]
mod tests;
