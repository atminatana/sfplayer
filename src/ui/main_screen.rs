//! The main screen of the window (spec v1.2 §3): the signal path top to bottom — MIDI sources, the
//! processor, the output, the audio file — and the status line pinned to the bottom row. Buttons are text
//! in square brackets: the screen says where each one goes, `Buttons` draws them and takes their input.
//! An error is shown red in the line of the setting it belongs to.
//!
//! Draws into a `ScreenBuffer` only, so the whole screen is testable without a window. All text goes
//! through `t!` (ADR 0008).

use std::time::Duration;

use gterm::buffer::ScreenBuffer;
use gterm_widgets::widgets::Meter;
use gterm_widgets::{BufferExt, Rect, Style, Widget};
use rust_i18n::t;

use super::app::{App, ButtonId};
use super::buttons::{Buttons, Look};
use super::meter::{dbfs_text, level_ratio};
use super::paint::Pen;
pub use super::paint::{BACKGROUND, DISABLED, ERROR, GOOD, HINT, LABEL, MESSAGE, METER_TRACK, RULE, TEXT, VALUE};
use super::piano;
use super::view::{FileView, NoticeTarget, TakeView, View, note_names, notice_target, notice_text};
use crate::config::{EngineKind, RecordFormat};
use crate::midi::MidiStatus;
use crate::midi::player::Transport;
use crate::session::MidiFile;
use crate::synth::{AdsrPreset, Wave};

/// The smallest grid the screen fits in (spec v1.2 §3); below it only a hint is drawn.
pub const MIN_COLS: u16 = 96;
pub const MIN_ROWS: u16 = 31;

/// Columns of a row: the label, an indented sub-label, and where values start (an 18-cell label column).
const LABEL_X: u16 = 1;
const SUB_LABEL_X: u16 = 3;
const VALUE_X: u16 = 19;
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

pub fn render(buf: &mut ScreenBuffer, app: &App, view: &View, buttons: &mut Buttons) {
	let locale = app.locale;
	buf.clear_all();
	buttons.begin_frame();
	let area = buf.area();
	if area.width < MIN_COLS || area.height < MIN_ROWS {
		let hint = t!("notice.window_too_small", locale = locale, width = MIN_COLS, height = MIN_ROWS);
		Pen::at(buf, 0, 0).text(&hint, ERROR);
		// No button is placed, so none reacts while the screen is not shown.
		buttons.finish_frame(buf);
		return;
	}

	title(buf, locale);
	sources(buf, SOURCES_Y, app, locale, view, buttons);
	let extra = match view.engine {
		EngineKind::Sf2 => 0,
		EngineKind::Builtin => SYNTH_EXTRA_ROWS,
	};
	handler(buf, HANDLER_Y, locale, view, buttons);
	output(buf, OUTPUT_Y + extra, app, locale, view, buttons);
	audio_file(buf, AUDIO_FILE_Y + extra, app, locale, view, buttons);
	status(buf, area.height - 1, locale, view);
	buttons.finish_frame(buf);
}

/// The top-left cell of the piano on a grid of `area`: centred, whatever the width (spec v1.2 §3).
/// `None` while the grid is too small for the screen to be shown.
pub fn piano_origin(area: Rect) -> Option<(u16, u16)> {
	let is_shown = area.width >= MIN_COLS && area.height >= MIN_ROWS;
	is_shown.then(|| (area.width.saturating_sub(piano::WIDTH) / 2, SOURCES_Y + PIANO_ROW))
}

/// Gives a button its place at the pen and moves the pen past it; the button is drawn when the frame is finished.
fn put(pen: &mut Pen<'_>, buttons: &mut Buttons, id: ButtonId, label: &str, look: Look) {
	let width = buttons.place(id, pen.x, pen.y, label, look);
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

/// The names of the sounding notes of one source from the value column up to column `end`, or a dash.
fn notes(buf: &mut ScreenBuffer, y: u16, keys: u128, end: u16) {
	let width = usize::from(end.saturating_sub(VALUE_X));
	match note_names(keys, width) {
		Some(names) => Pen::at(buf, VALUE_X, y).text(&names, VALUE),
		None => Pen::at(buf, VALUE_X, y).text(NOTHING, DISABLED),
	};
}

fn sources(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, buttons: &mut Buttons) {
	let after_title = section(buf, y, &t!("main.section_sources", locale = locale));
	rule(buf, after_title, y);

	label(buf, y + 1, &t!("main.keyboard", locale = locale));
	// The names stop two cells short of the octave and velocity read-out.
	notes(buf, y + 1, view.keyboard_notes, KEYBOARD_STATE_X - 2);
	let keyboard_state = t!(
		"status.octave",
		locale = locale,
		octave = app.keyboard.octave(),
		velocity = format!("{:>3}", app.keyboard.velocity())
	);
	Pen::at(buf, KEYBOARD_STATE_X, y + 1).text(&keyboard_state, HINT);
	if let Some((piano_x, piano_y)) = piano_origin(buf.area()) {
		let sounding = view.keyboard_notes | view.midi_notes;
		piano::draw(buf, piano_x, piano_y, app.keyboard.base_key(), sounding);
	}

	let midi_in = t!("main.midi_in", locale = locale);
	label(buf, y + 4, &midi_in);
	let mut pen = Pen::at(buf, VALUE_X, y + 4);
	put(&mut pen, buttons, ButtonId::MidiIn, &midi_in, Look::Normal);
	pen.gap(2);
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
	let right_edge = buf.area().width.saturating_sub(1);
	notes(buf, y + 5, view.midi_notes, right_edge);

	midi_file(buf, y + 6, locale, &view.file, buttons);
}

/// The MIDI file row — the file, or why there is none to play, and the transport buttons at the right
/// end — and under it, once a file is chosen, where its playback is.
fn midi_file(buf: &mut ScreenBuffer, y: u16, locale: &str, file: &FileView, buttons: &mut Buttons) {
	let title = t!("main.midi_file", locale = locale);
	label(buf, y, &title);
	let transport = [
		(ButtonId::FileStart, t!("main.start", locale = locale)),
		(ButtonId::FilePause, t!("main.pause", locale = locale)),
		(ButtonId::FileStop, t!("main.stop", locale = locale)),
	];
	// Brackets and the gap before each of the three.
	let transport_width: usize = transport.iter().map(|(_, name)| name.chars().count() + 3).sum();
	let mut pen = Pen::at(buf, VALUE_X, y);
	put(&mut pen, buttons, ButtonId::MidiFile, &title, Look::Normal);
	pen.gap(2);
	// What is said about the file ends where the buttons begin, one cell short of the right edge.
	let room = usize::from(buf_width(&pen)).saturating_sub(usize::from(pen.x) + transport_width + 2);
	let is_loaded = matches!(file.file, MidiFile::Loaded(_));
	match &file.file {
		MidiFile::None => pen.text(&t!("main.no_file", locale = locale), HINT),
		MidiFile::Loaded(path) => pen.text(&fitted(&file_name(path), room), TEXT),
		MidiFile::Missing(path) => {
			let missing = t!("main.file_missing", locale = locale, path = path.display());
			pen.text(&fitted(&missing, room), ERROR)
		}
		MidiFile::Broken { reason, .. } => pen.text(&fitted(reason, room), ERROR),
	};
	pen.gap(1);
	for (id, name) in &transport {
		let is_available = is_loaded
			&& match id {
				ButtonId::FileStart => file.transport != Transport::Playing,
				ButtonId::FilePause => file.transport == Transport::Playing,
				_ => file.transport != Transport::Stopped,
			};
		pen.gap(1);
		put(
			&mut pen,
			buttons,
			*id,
			name,
			if is_available { Look::Normal } else { Look::Disabled },
		);
	}
	if !is_loaded {
		return;
	}

	sub_label(buf, y + 1, &t!("main.position", locale = locale));
	let ratio = if file.duration_tenths == 0 {
		0.0
	} else {
		f64::from(file.position_tenths) / f64::from(file.duration_tenths)
	};
	Pen::at(buf, VALUE_X, y + 1).text("[", LABEL);
	Meter::new()
		.ratio(ratio)
		.zones(&[(0.0, Style::new().fg(VALUE))])
		.unfilled_symbol(POSITION_TRACK)
		.unfilled_style(Style::new().fg(METER_TRACK))
		.render(Rect::new(VALUE_X + 1, y + 1, POSITION_CELLS, 1), buf);
	let clock = |tenths: u32| format!("{:02}:{:02}", tenths / 600, tenths / 10 % 60);
	let (state, colour) = match file.transport {
		Transport::Playing => (t!("main.playing", locale = locale), GOOD),
		Transport::Paused => (t!("main.paused", locale = locale), MESSAGE),
		Transport::Stopped => (t!("main.stopped", locale = locale), HINT),
	};
	Pen::at(buf, VALUE_X + 1 + POSITION_CELLS, y + 1)
		.text("]", LABEL)
		.gap(2)
		.text(&format!("{} / {}", clock(file.position_tenths), clock(file.duration_tenths)), TEXT)
		.gap(2)
		.text(&state, colour);
}

/// The name of a file without its folder.
fn file_name(path: &std::path::Path) -> String {
	path.file_name()
		.map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// `text` cut to `room` cells; what is cut off is marked with `…`.
fn fitted(text: &str, room: usize) -> String {
	if text.chars().count() <= room {
		return text.to_owned();
	}
	let kept: String = text.chars().take(room.saturating_sub(1)).collect();
	format!("{kept}…")
}

/// The last `room` cells of `text`; what is cut off at the front is marked with `…`.
fn fitted_end(text: &str, room: usize) -> String {
	let length = text.chars().count();
	if length <= room {
		return text.to_owned();
	}
	let kept: String = text.chars().skip(length - room.saturating_sub(1)).collect();
	format!("…{kept}")
}

/// The width of the grid the pen writes into.
fn buf_width(pen: &Pen<'_>) -> u16 {
	pen.clip_width()
}

/// "PROCESSOR  [Synth] [Sound Font] ──" and, under it, the rows of the chosen processor; both end
/// with the effects row, which the two share.
fn handler(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View, buttons: &mut Buttons) {
	let after_title = section(buf, y, &t!("main.section_handler", locale = locale));
	let chosen = |kind: EngineKind| if view.engine == kind { Look::Active } else { Look::Normal };
	let mut pen = Pen::at(buf, after_title + 1, y);
	put(
		&mut pen,
		buttons,
		ButtonId::EngineSynth,
		&t!("main.engine_synth", locale = locale),
		chosen(EngineKind::Builtin),
	);
	pen.gap(1);
	put(
		&mut pen,
		buttons,
		ButtonId::EngineSf2,
		&t!("main.engine_sf2", locale = locale),
		chosen(EngineKind::Sf2),
	);
	pen.gap(1);
	let after_buttons = pen.x;
	rule(buf, after_buttons, y);

	let effects_y = match view.engine {
		EngineKind::Sf2 => {
			sound_font(buf, y + 1, locale, view, buttons);
			y + 3
		}
		EngineKind::Builtin => {
			synth(buf, y + 1, locale, view, buttons);
			y + 3 + SYNTH_EXTRA_ROWS
		}
	};
	let effects = t!("main.effects", locale = locale);
	label(buf, effects_y, &effects);
	let off = t!("mixer.off", locale = locale);
	let level = |value: Option<u8>| value.map_or_else(|| off.to_string(), |level| level.to_string());
	let sends = format!(
		"{} {}  {} {}",
		t!("mixer.reverb", locale = locale),
		level(view.reverb),
		t!("mixer.chorus", locale = locale),
		level(view.chorus)
	);
	let mut pen = Pen::at(buf, VALUE_X, effects_y);
	put(&mut pen, buttons, ButtonId::Effects, &effects, Look::Normal);
	pen.gap(2).text(&sends, TEXT);
}

/// The four rows of the built-in synthesizer: the wave, the ready-made envelopes, the envelope's values.
fn synth(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View, buttons: &mut Buttons) {
	let timbre = &view.builtin;
	label(buf, y, &t!("main.wave", locale = locale));
	let mut pen = Pen::at(buf, VALUE_X, y);
	for (id, wave, key) in [
		(ButtonId::WaveSine, Wave::Sine, "main.wave_sine"),
		(ButtonId::WaveSaw, Wave::Saw, "main.wave_saw"),
		(ButtonId::WaveTriangle, Wave::Triangle, "main.wave_triangle"),
	] {
		let look = if timbre.wave == wave { Look::Active } else { Look::Normal };
		put(&mut pen, buttons, id, &t!(key, locale = locale), look);
		pen.gap(1);
	}

	label(buf, y + 1, "ADSR");
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	pen.text(&t!("main.adsr_preset", locale = locale), LABEL).gap(1);
	for (id, preset, key) in [
		(ButtonId::AdsrPiano, AdsrPreset::Piano, "main.adsr_piano"),
		(ButtonId::AdsrHarpsichord, AdsrPreset::Harpsichord, "main.adsr_harpsichord"),
		(ButtonId::AdsrOrgan, AdsrPreset::Organ, "main.adsr_organ"),
		(ButtonId::AdsrFlute, AdsrPreset::Flute, "main.adsr_flute"),
	] {
		let look = if timbre.adsr_preset == preset { Look::Active } else { Look::Normal };
		put(&mut pen, buttons, id, &t!(key, locale = locale), look);
		pen.gap(1);
	}
	// Values set by hand belong to no preset (spec v1.2, F-22).
	if timbre.adsr_preset == AdsrPreset::Custom {
		pen.gap(1).text(&t!("main.adsr_custom", locale = locale), VALUE);
	}

	sub_label(buf, y + 2, &t!("main.adsr_values", locale = locale));
	let seconds = |value: f32| t!("main.seconds", locale = locale, value = format!("{value:.3}")).into_owned();
	let per_cent = |share: f32| format!("{:>3}%", (share * 100.0).round() as u32);
	for (row, x, name, value, down, up) in [
		(
			y + 2,
			VALUE_X,
			"Attack:",
			seconds(timbre.attack),
			ButtonId::AttackDown,
			ButtonId::AttackUp,
		),
		(
			y + 2,
			SECOND_VALUE_X,
			"Decay:",
			seconds(timbre.decay),
			ButtonId::DecayDown,
			ButtonId::DecayUp,
		),
		(
			y + 3,
			VALUE_X,
			"Sustain:",
			per_cent(timbre.sustain),
			ButtonId::SustainDown,
			ButtonId::SustainUp,
		),
		(
			y + 3,
			SECOND_VALUE_X,
			"Release:",
			seconds(timbre.release),
			ButtonId::ReleaseDown,
			ButtonId::ReleaseUp,
		),
	] {
		let mut pen = Pen::at(buf, x, row);
		pen.text(name, LABEL).gap(1);
		put(&mut pen, buttons, down, MINUS, Look::Normal);
		pen.gap(1).text(&value, VALUE).gap(1);
		put(&mut pen, buttons, up, PLUS, Look::Normal);
	}
}

/// The two rows of a SoundFont: the bank file and the preset.
fn sound_font(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View, buttons: &mut Buttons) {
	let sf2_file = t!("main.sf2_file", locale = locale);
	label(buf, y, &sf2_file);
	let mut pen = Pen::at(buf, VALUE_X, y);
	put(&mut pen, buttons, ButtonId::Sf2File, &sf2_file, Look::Normal);
	pen.gap(2);
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
	label(buf, y + 1, &preset);
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	match &view.bank {
		Some(bank) => {
			put(&mut pen, buttons, ButtonId::Preset, &preset, Look::Normal);
			pen.gap(2).text(&bank.preset_label, TEXT);
		}
		None => {
			put(&mut pen, buttons, ButtonId::Preset, &preset, Look::Disabled);
			pen.gap(2).text(NOTHING, DISABLED);
		}
	}
}

fn output(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, buttons: &mut Buttons) {
	let after_title = section(buf, y, &t!("main.section_output", locale = locale));
	rule(buf, after_title, y);

	let audio_out = t!("main.audio_out", locale = locale);
	label(buf, y + 1, &audio_out);
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	put(&mut pen, buttons, ButtonId::AudioOut, &audio_out, Look::Normal);
	pen.gap(2);
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
	put(&mut pen, buttons, ButtonId::MasterDown, MINUS, Look::Normal);
	pen.gap(1);
	let meter_x = pen.x;
	level_meter(buf, meter_x, y + 4, MASTER_CELLS, f64::from(mixer.master) / 100.0);
	let mut pen = Pen::at(buf, meter_x + MASTER_CELLS + 1, y + 4);
	pen.text(&format!("{:>3}%", mixer.master), VALUE).gap(1);
	put(&mut pen, buttons, ButtonId::MasterUp, PLUS, Look::Normal);
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

fn audio_file(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, buttons: &mut Buttons) {
	let after_title = section(buf, y, &t!("main.section_record", locale = locale));
	rule(buf, after_title, y);
	let recording = app.recording;

	label(buf, y + 1, &t!("main.folder", locale = locale));
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	put(
		&mut pen,
		buttons,
		ButtonId::Folder,
		&t!("main.folder_button", locale = locale),
		Look::Normal,
	);
	pen.gap(2).text(&view.record_dir, TEXT);

	label(buf, y + 2, &t!("main.format", locale = locale));
	// A running take keeps its format (spec v1.2, F-23): the chosen one stays lit, none reacts.
	let format_look = |format: RecordFormat| match (view.record_format == format, recording.is_some()) {
		(true, false) => Look::Active,
		(true, true) => Look::ActiveLocked,
		(false, false) => Look::Normal,
		(false, true) => Look::Disabled,
	};
	let mut pen = Pen::at(buf, VALUE_X, y + 2);
	put(&mut pen, buttons, ButtonId::FormatFlac, "FLAC", format_look(RecordFormat::Flac));
	pen.gap(1);
	put(&mut pen, buttons, ButtonId::FormatOgg, "OGG", format_look(RecordFormat::OggVorbis));
	pen.gap(1);
	// Recording MIDI arrives with plan stage 6; until then that format is shown unavailable.
	put(&mut pen, buttons, ButtonId::FormatMidi, "MIDI", Look::Disabled);
	pen.gap(2).text(&t!("main.format_audio", locale = locale), HINT);

	label(buf, y + 3, &t!("main.record", locale = locale));
	let unless = |unavailable: bool| if unavailable { Look::Disabled } else { Look::Normal };
	let mut pen = Pen::at(buf, VALUE_X, y + 3);
	put(
		&mut pen,
		buttons,
		ButtonId::RecordStart,
		&t!("main.start", locale = locale),
		unless(recording.is_some()),
	);
	pen.gap(1);
	put(
		&mut pen,
		buttons,
		ButtonId::RecordStop,
		&t!("main.stop", locale = locale),
		unless(recording.is_none()),
	);
	pen.gap(1);
	// Cancelling throws the take away: the button is red while there is a take to throw away.
	put(
		&mut pen,
		buttons,
		ButtonId::RecordCancel,
		&t!("main.cancel", locale = locale),
		if recording.is_some() { Look::Danger } else { Look::Disabled },
	);
	pen.gap(2);
	// What follows the buttons has the row up to one cell short of the right edge.
	let room = usize::from(buf_width(&pen).saturating_sub(pen.x + 1));
	let megabytes = |tenths: u64| {
		t!(
			"main.megabytes",
			locale = locale,
			value = format!("{}.{}", tenths / 10, tenths % 10)
		)
		.into_owned()
	};
	match (recording, view.line_error(NoticeTarget::Record)) {
		(Some(elapsed), _) => {
			let time = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);
			let mut indicator = format!("{} {}", record_dot(elapsed), t!("status.rec", locale = locale, time = time));
			// The size and the name of the file being written follow the timer (spec v1.2 §3).
			if let TakeView::Recording { name, tenths_of_megabyte } = &view.take {
				indicator = format!("{indicator} · {} · {name}", megabytes(*tenths_of_megabyte));
			}
			pen.styled(&fitted(&indicator, room), Style::new().fg(ERROR).bold());
		}
		(None, Some(error)) => {
			pen.text(&fitted(error, room), ERROR);
		}
		(None, None) => match &view.take {
			TakeView::Saved { path, tenths_of_megabyte } => {
				// A path too long for the row loses its beginning: the file name is what tells takes apart.
				let size = megabytes(*tenths_of_megabyte);
				let frame = t!("main.saved", locale = locale, path = "", size = size).chars().count();
				let saved = t!(
					"main.saved",
					locale = locale,
					path = fitted_end(path, room.saturating_sub(frame)),
					size = size
				);
				pen.text(&saved, GOOD);
			}
			TakeView::None | TakeView::Recording { .. } => {
				pen.text(&t!("main.no_takes", locale = locale), HINT);
			}
		},
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
