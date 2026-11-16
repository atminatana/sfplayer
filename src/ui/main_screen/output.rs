//! The "Output" and "Audio file" sections of the main screen: the audio device, the level meters and
//! the master volume; the folder and the format of takes and the row of the take itself.

use std::time::Duration;

use gterm::buffer::ScreenBuffer;
use gterm_widgets::widgets::Meter;
use gterm_widgets::{Rect, Style, Widget};
use rust_i18n::t;

use super::{
	ERROR, GOOD, HINT, LABEL_X, MASTER_CELLS, MESSAGE, METER_CELLS, METER_TRACK, METER_ZONES, MINUS, PLUS, RECORD_BLINK_HALF_PERIOD_MS,
	RECORD_DOT, TEXT, VALUE, VALUE_X, buf_width, fitted, label, put, rule, section,
};
use crate::config::RecordFormat;
use crate::ui::app::{App, ButtonId};
use crate::ui::buttons::{Buttons, Look};
use crate::ui::meter::{dbfs_text, level_ratio};
use crate::ui::paint::Pen;
use crate::ui::view::{NoticeTarget, Size, TakeView, View, notice_target, notice_text};

/// The last `room` cells of `text`; what is cut off at the front is marked with `…`.
pub(super) fn fitted_end(text: &str, room: usize) -> String {
	let length = text.chars().count();
	if length <= room {
		return text.to_owned();
	}
	let kept: String = text.chars().skip(length - room.saturating_sub(1)).collect();
	format!("…{kept}")
}

pub(super) fn output(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, buttons: &mut Buttons) {
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

pub(super) fn audio_file(buf: &mut ScreenBuffer, y: u16, app: &App, locale: &str, view: &View, buttons: &mut Buttons) {
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
	put(&mut pen, buttons, ButtonId::FormatMidi, "MIDI", format_look(RecordFormat::Midi));
	let recorded = match view.record_format {
		RecordFormat::Flac | RecordFormat::OggVorbis => t!("main.format_audio", locale = locale),
		RecordFormat::Midi => t!("main.format_midi", locale = locale),
	};
	pen.gap(2).text(&recorded, HINT);

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
	let size_text = |size: Size| match size {
		Size::Kilobytes(kilobytes) => t!("main.kilobytes", locale = locale, value = kilobytes).into_owned(),
		Size::TenthsOfMegabyte(tenths) => t!(
			"main.megabytes",
			locale = locale,
			value = format!("{}.{}", tenths / 10, tenths % 10)
		)
		.into_owned(),
	};
	match (recording, view.line_error(NoticeTarget::Record)) {
		(Some(elapsed), _) => {
			let time = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);
			let mut indicator = format!("{} {}", record_dot(elapsed), t!("status.rec", locale = locale, time = time));
			// The size and the name of the file being written follow the timer (spec v1.2 §3).
			if let TakeView::Recording { name, size } = &view.take {
				indicator = format!("{indicator} · {} · {name}", size_text(*size));
			}
			pen.styled(&fitted(&indicator, room), Style::new().fg(ERROR).bold());
		}
		(None, Some(error)) => {
			pen.text(&fitted(error, room), ERROR);
		}
		(None, None) => match &view.take {
			TakeView::Saved { path, size } => {
				// A path too long for the row loses its beginning: the file name is what tells takes apart.
				let size = size_text(*size);
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
pub(super) fn record_dot(elapsed: Duration) -> &'static str {
	if (elapsed.as_millis() / RECORD_BLINK_HALF_PERIOD_MS).is_multiple_of(2) {
		RECORD_DOT
	} else {
		" "
	}
}
