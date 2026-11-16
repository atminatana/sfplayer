//! The "MIDI sources" section of the main screen: the text keyboard with the piano, the MIDI input
//! and the MIDI file with its transport and position.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::widgets::Meter;
use gterm_widgets::{BufferExt, Rect, Style, Widget};
use rust_i18n::t;

use super::{
	DEVICE_CELLS, DISABLED, ERROR, GOOD, HINT, KEYBOARD_STATE_X, LABEL, MESSAGE, METER_TRACK, NOTHING, POSITION_CELLS, POSITION_TRACK,
	TEXT, VALUE, VALUE_X, buf_width, fitted, label, piano_origin, put, rule, section, sub_label,
};
use crate::midi::MidiStatus;
use crate::midi::player::Transport;
use crate::session::MidiFile;
use crate::ui::app::{App, ButtonId, Dropdown};
use crate::ui::controls::{Controls, Look};
use crate::ui::paint::Pen;
use crate::ui::piano;
use crate::ui::view::{FileView, NoticeTarget, View, note_names};

/// The names of the sounding notes of one source from the value column up to column `end`, or a dash.
fn notes(buf: &mut ScreenBuffer, y: u16, keys: u128, end: u16) {
	let width = usize::from(end.saturating_sub(VALUE_X));
	match note_names(keys, width) {
		Some(names) => Pen::at(buf, VALUE_X, y).text(&names, VALUE),
		None => Pen::at(buf, VALUE_X, y).text(NOTHING, DISABLED),
	};
}

pub(super) fn sources(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, controls: &mut Controls) {
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

	label(buf, y + 4, &t!("main.midi_in", locale = locale));
	// The port is chosen in place: the field shows the one in use, or says that there is none.
	let (port, absent) = match &view.midi {
		MidiStatus::Connected(port) => (Some(port.as_str()), String::new()),
		MidiStatus::Disconnected(port) => (None, t!("header.disconnected", locale = locale, port = port).into_owned()),
		MidiStatus::NoPort => (None, t!("header.no_device", locale = locale).into_owned()),
	};
	let field = controls.place_dropdown(Dropdown::MidiIn, VALUE_X, y + 4, DEVICE_CELLS, port, &absent);
	let mut pen = Pen::at(buf, VALUE_X + field + 2, y + 4);
	if port.is_some() {
		pen.text(&t!("main.opened", locale = locale), GOOD).gap(2);
	}
	if let Some(error) = view.line_error(NoticeTarget::Midi) {
		pen.text(error, ERROR);
	}
	sub_label(buf, y + 5, &t!("main.notes", locale = locale));
	let right_edge = buf.area().width.saturating_sub(1);
	notes(buf, y + 5, view.midi_notes, right_edge);

	midi_file(buf, y + 6, locale, &view.file, controls);
}

/// The MIDI file row — the file, or why there is none to play, and the transport buttons at the right
/// end — and under it, once a file is chosen, where its playback is.
fn midi_file(buf: &mut ScreenBuffer, y: u16, locale: &str, file: &FileView, controls: &mut Controls) {
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
	put(&mut pen, controls, ButtonId::MidiFile, &title, Look::Normal);
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
			controls,
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
