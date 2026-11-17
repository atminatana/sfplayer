//! The "Output" and "Recording" sections of the main screen: the audio device, the level meters and
//! the master volume; the folder and the format of takes and the row of the take itself.

use std::time::Duration;

use gterm::buffer::ScreenBuffer;
use gterm_widgets::widgets::Meter;
use gterm_widgets::{BufferExt, Rect, Style, Widget};
use rust_i18n::t;

use super::{
	DEVICE_CELLS, ERROR, FORMAT_CELLS, GOOD, HINT, LABEL, LABEL_X, MASTER_CELLS, MESSAGE, METER_CELLS, METER_TRACK, METER_ZONES, MINUS,
	PLUS, RECORD_BLINK_HALF_PERIOD_MS, RECORD_DOT, SECTION_HEAD_ROWS, TEXT, VALUE, VALUE_X, content_end, fitted, label, put, rule, section,
};
use crate::config::RecordFormat;
use crate::ui::app::{App, ButtonId, Dropdown, RECORD_FORMATS};
use crate::ui::controls::{Controls, Look};
use crate::ui::meter::{dbfs_text, level_ratio};
use crate::ui::paint::Pen;
use crate::ui::view::{NoticeTarget, TakeView, View, clock_text, notice_target, notice_text};

/// The last `room` cells of `text`; what is cut off at the front is marked with `…`.
pub(super) fn fitted_end(text: &str, room: usize) -> String {
	let length = text.chars().count();
	if length <= room {
		return text.to_owned();
	}
	let kept: String = text.chars().skip(length - room.saturating_sub(1)).collect();
	format!("…{kept}")
}

pub(super) fn output(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, controls: &mut Controls) {
	let after_title = section(buf, y, &t!("main.section_output", locale = locale));
	rule(buf, after_title, y);
	let device_y = y + SECTION_HEAD_ROWS;
	let master_y = device_y + 3;

	label(buf, device_y, &t!("main.audio_out", locale = locale));
	// The device is chosen in place: the field shows the one in use, or says that there is none.
	let device = view.audio.as_ref().map(|audio| audio.device.as_str());
	let no_device = t!("header.no_device", locale = locale);
	let field = controls.place_dropdown(Dropdown::AudioOut, VALUE_X, device_y, DEVICE_CELLS, device, &no_device);
	let mut pen = Pen::at(buf, VALUE_X + field + 2, device_y);
	// An error takes the room of the rate and the buffer: while it is there, it is what matters.
	match (view.line_error(NoticeTarget::Audio), &view.audio) {
		(Some(error), _) => {
			pen.text(error, ERROR);
		}
		(None, Some(audio)) => {
			let buffer = match audio.buffer {
				Some(frames) => t!("header.buffer", locale = locale, frames = frames),
				None => t!("header.buffer_default", locale = locale),
			};
			pen.text(&format!("{} Hz / {buffer}", audio.sample_rate), HINT)
				.gap(2)
				.text(&t!("header.audio_ok", locale = locale), GOOD);
		}
		(None, None) => {}
	}

	let silence = t!("meter.silence", locale = locale);
	for (row, name, peak, rms) in [
		(device_y + 1, "L", view.levels.peak_left, view.levels.rms_left),
		(device_y + 2, "R", view.levels.peak_right, view.levels.rms_right),
	] {
		label(buf, row, name);
		level_meter(buf, VALUE_X, row, METER_CELLS, level_ratio(rms));
		let mut pen = Pen::at(buf, VALUE_X + METER_CELLS + 2, row);
		pen.text(&dbfs_text(peak, &silence), TEXT);
		if app.clipping {
			pen.gap(2).styled(&t!("meter.clip", locale = locale), Style::new().fg(ERROR).bold());
		}
	}

	label(buf, master_y, &t!("main.master", locale = locale));
	let mixer = &view.mixer;
	let mut pen = Pen::at(buf, VALUE_X, master_y);
	put(&mut pen, controls, ButtonId::MasterDown, MINUS, Look::Normal);
	pen.gap(1);
	let meter_x = pen.x;
	level_meter(buf, meter_x, master_y, MASTER_CELLS, f64::from(mixer.master) / 100.0);
	let mut pen = Pen::at(buf, meter_x + MASTER_CELLS + 1, master_y);
	pen.text(&format!("{:>3}%", mixer.master), VALUE).gap(1);
	put(&mut pen, controls, ButtonId::MasterUp, PLUS, Look::Normal);
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

/// The label of a recording row: with a colon, its end next to the value column.
fn record_label(buf: &mut ScreenBuffer, y: u16, text: &str) {
	let text = format!("{text}:");
	let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
	Pen::at(buf, (VALUE_X - 1).saturating_sub(width), y).text(&text, LABEL);
}

pub(super) fn audio_file(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, controls: &mut Controls) {
	let after_title = section(buf, y, &t!("main.section_record", locale = locale));
	rule(buf, after_title, y);
	let recording = app.recording;
	let folder_y = y + SECTION_HEAD_ROWS;
	let format_y = folder_y + 1;
	let take_y = folder_y + 2;

	record_label(buf, folder_y, &t!("main.folder_button", locale = locale));
	let mut pen = Pen::at(buf, VALUE_X, folder_y);
	// The button is the folder itself: pressing it asks for another one. A path too long for the row
	// loses its beginning; the brackets of the button take two cells.
	let room = usize::from(content_end(pen.clip_width()).saturating_sub(VALUE_X + 2));
	put(
		&mut pen,
		controls,
		ButtonId::Folder,
		&fitted_end(&view.record_dir, room),
		Look::Normal,
	);

	record_label(buf, format_y, &t!("main.format", locale = locale));
	let chosen = RECORD_FORMATS
		.iter()
		.find(|(format, _)| *format == view.record_format)
		.map(|(_, name)| *name);
	let field = controls.place_dropdown(Dropdown::Format, VALUE_X, format_y, FORMAT_CELLS, chosen, "");
	// A running take keeps its format (spec v1.2, F-23): the field shows it and does not open.
	if recording.is_some() {
		controls.lock_dropdown(Dropdown::Format);
	}
	let recorded = match view.record_format {
		RecordFormat::Flac | RecordFormat::OggVorbis => t!("main.format_audio", locale = locale),
		RecordFormat::Midi => t!("main.format_midi", locale = locale),
	};
	Pen::at(buf, VALUE_X + field + 2, format_y).text(&recorded, HINT);

	record_label(buf, take_y, &t!("main.record", locale = locale));
	let unless = |unavailable: bool| if unavailable { Look::Disabled } else { Look::Normal };
	let mut pen = Pen::at(buf, VALUE_X, take_y);
	put(
		&mut pen,
		controls,
		ButtonId::RecordStart,
		&t!("main.start", locale = locale),
		unless(recording.is_some()),
	);
	pen.gap(1);
	put(
		&mut pen,
		controls,
		ButtonId::RecordStop,
		&t!("main.stop", locale = locale),
		unless(recording.is_none()),
	);
	pen.gap(1);
	// Cancelling throws the take away: the button is red while there is a take to throw away.
	put(
		&mut pen,
		controls,
		ButtonId::RecordCancel,
		&t!("main.cancel", locale = locale),
		if recording.is_some() { Look::Danger } else { Look::Disabled },
	);
	pen.gap(2);
	// What follows the buttons has the row up to the end of the content.
	let room = usize::from(content_end(pen.clip_width()).saturating_sub(pen.x));
	match (recording, view.line_error(NoticeTarget::Record)) {
		(Some(elapsed), _) => {
			let time = clock_text(elapsed);
			let mut indicator = format!("{} {}", record_dot(elapsed), t!("status.rec", locale = locale, time = time));
			// The size and the name of the file being written follow the timer (spec v1.2 §3).
			if let TakeView::Recording { name, size } = &view.take {
				indicator = format!("{indicator} · {} · {name}", size.text(locale));
			}
			pen.styled(&fitted(&indicator, room), Style::new().fg(ERROR).bold());
		}
		(None, Some(error)) => {
			pen.text(&fitted(error, room), ERROR);
		}
		(None, None) => match &view.take {
			TakeView::Saved { path, size } => {
				// A path too long for the row loses its beginning: the file name is what tells takes apart.
				let size = size.text(locale);
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
		let room = usize::from(content_end(buf.area().width).saturating_sub(LABEL_X));
		Pen::at(buf, LABEL_X, take_y + 1).text(&fitted(&notice_text(notice, locale), room), MESSAGE);
	}
}

/// The recording dot, lit on even half-periods of the take's elapsed time; a space keeps the width constant.
pub(super) fn record_dot(elapsed: Duration) -> &'static str {
	if (elapsed.as_millis() / RECORD_BLINK_HALF_PERIOD_MS).is_multiple_of(2) {
		RECORD_DOT
	} else {
		" "
	}
}
