//! Drawing the main screen (spec §6 mock-up). All text goes through `t!` (ADR 0008).

use cpal::BufferSize;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use rust_i18n::t;

use super::app::{App, Panel};
use crate::midi::MidiStatus;
use crate::session::{Notice, Session};

/// Spec §6: the layout is designed for at least 80×24.
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

/// Everything the screen shows, decoupled from live handles so rendering is testable.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
	pub midi: MidiStatus,
	pub audio: Option<AudioView>,
	pub bank: Option<BankView>,
	pub xruns: u32,
	pub dropped: u32,
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
	pub name: String,
	pub presets: usize,
	/// "bank:program name" of the preset on the current channel.
	pub preset_label: String,
}

impl View {
	pub fn from_session(session: &Session) -> Self {
		let audio = session.audio().map(|stream| AudioView {
			device: stream.device_name.clone(),
			sample_rate: stream.sample_rate,
			buffer: stream.buffer_size,
		});
		let bank = session.bank.as_ref().map(|loaded| {
			let presets = loaded.bank.presets();
			let preset_label = presets
				.first()
				.map(|preset| format!("{:03}:{:03} {}", preset.bank, preset.program, preset.name))
				.unwrap_or_default();
			BankView {
				name: loaded.bank.name().to_owned(),
				presets: presets.len(),
				preset_label,
			}
		});
		Self {
			midi: session.midi_status.clone(),
			audio,
			bank,
			xruns: session.shared.xruns(),
			dropped: session.midi_dropped.load(std::sync::atomic::Ordering::Relaxed),
			notice: session.notice.clone(),
		}
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

	let outer = Block::default().borders(Borders::ALL).title(
		t!("app.title", locale = locale, version = env!("CARGO_PKG_VERSION"))
			.as_ref()
			.to_owned(),
	);
	let inner = outer.inner(area);
	frame.render_widget(outer, area);

	let rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(2), Constraint::Min(5), Constraint::Length(4)])
		.split(inner);
	render_header(frame, rows[0], locale, view);
	render_panels(frame, rows[1], app, locale);
	render_status(frame, rows[2], locale, view);
}

fn render_header(frame: &mut Frame, area: Rect, locale: &str, view: &View) {
	let none = t!("header.none", locale = locale);
	let midi_text = match &view.midi {
		MidiStatus::Connected(port) => format!("{port} ●"),
		MidiStatus::Disconnected(port) => t!("header.disconnected", locale = locale, port = port).into_owned(),
		MidiStatus::NoPort => none.to_string(),
	};
	let audio_text = match &view.audio {
		Some(audio) => {
			let buffer = match audio.buffer {
				BufferSize::Fixed(frames) => t!("header.buffer", locale = locale, frames = frames).into_owned(),
				BufferSize::Default => t!("header.buffer_default", locale = locale).into_owned(),
			};
			format!("{} {} Hz / {buffer}", audio.device, audio.sample_rate)
		}
		None => none.to_string(),
	};
	let line1 = Line::from(vec![
		label(t!("header.midi", locale = locale)),
		Span::raw(format!(" {midi_text}   ")),
		label(t!("header.audio", locale = locale)),
		Span::raw(format!(" {audio_text}")),
	]);

	let line2 = match &view.bank {
		Some(bank) => Line::from(vec![
			label(t!("header.sf2", locale = locale)),
			Span::raw(format!(
				" {} {}   ",
				bank.name,
				t!("header.presets", locale = locale, count = bank.presets)
			)),
			label(t!("header.preset", locale = locale, channel = 1)),
			Span::raw(format!(" {}", bank.preset_label)),
		]),
		None => Line::from(vec![
			label(t!("header.sf2", locale = locale)),
			Span::raw(format!(" {}", t!("header.no_sf2", locale = locale))),
		]),
	};
	frame.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn render_panels(frame: &mut Frame, area: Rect, app: &App, locale: &str) {
	let columns = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
		.split(area);
	frame.render_widget(
		panel_block(t!("panel.monitor", locale = locale).as_ref(), app.focus == Panel::Monitor),
		columns[0],
	);
	frame.render_widget(
		panel_block(t!("panel.mixer", locale = locale).as_ref(), app.focus == Panel::Mixer),
		columns[1],
	);
}

fn render_status(frame: &mut Frame, area: Rect, locale: &str, view: &View) {
	let block = panel_block(t!("panel.status", locale = locale).as_ref(), false);
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let first = match &view.notice {
		Some(notice) => Line::from(Span::styled(notice_text(notice, locale), Style::default().fg(Color::Yellow))),
		None => {
			let mut counters = vec![
				t!("status.voices", locale = locale, voices = "-/-").into_owned(),
				t!("status.cpu", locale = locale, percent = "-").into_owned(),
				t!("status.xruns", locale = locale, count = view.xruns).into_owned(),
			];
			if view.dropped > 0 {
				counters.push(t!("status.dropped", locale = locale, count = view.dropped).into_owned());
			}
			Line::from(counters.join("  "))
		}
	};
	let hints = ["hint.help", "hint.midi", "hint.audio", "hint.sf2", "hint.record", "hint.quit"]
		.iter()
		.map(|key| t!(*key, locale = locale).into_owned())
		.collect::<Vec<_>>()
		.join(" ");
	frame.render_widget(Paragraph::new(vec![first, Line::from(hints)]), inner);
}

fn notice_text(notice: &Notice, locale: &str) -> String {
	match notice {
		Notice::NoAudioDevice(error) => t!("notice.no_audio_device", locale = locale, error = error).into_owned(),
		Notice::AudioFallback => t!("notice.audio_fallback", locale = locale).into_owned(),
		Notice::MidiFallback => t!("notice.midi_fallback", locale = locale).into_owned(),
		Notice::MidiError(error) => t!("notice.midi_error", locale = locale, error = error).into_owned(),
		Notice::Sf2Error(error) => t!("notice.sf2_error", locale = locale, error = error).into_owned(),
	}
}

fn label<'a>(text: impl Into<std::borrow::Cow<'a, str>>) -> Span<'a> {
	Span::styled(text.into(), Style::default().add_modifier(Modifier::BOLD))
}

fn panel_block(title: &str, focused: bool) -> Block<'static> {
	let style = if focused {
		Style::default().fg(Color::Cyan)
	} else {
		Style::default()
	};
	Block::default().borders(Borders::ALL).border_style(style).title(title.to_owned())
}

#[cfg(test)]
mod tests {
	use ratatui::Terminal;
	use ratatui::backend::TestBackend;

	use super::*;

	fn view() -> View {
		View {
			midi: MidiStatus::Connected("Arturia KeyStep".into()),
			audio: Some(AudioView {
				device: "Speakers".into(),
				sample_rate: 48_000,
				buffer: BufferSize::Fixed(256),
			}),
			bank: Some(BankView {
				name: "GeneralUser GS".into(),
				presets: 268,
				preset_label: "000:000 Grand Piano".into(),
			}),
			xruns: 0,
			dropped: 0,
			notice: None,
		}
	}

	fn draw(locale: &'static str, view: &View, width: u16, height: u16) -> Vec<String> {
		let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
		let app = App::new(locale);
		terminal.draw(|frame| render(frame, &app, view)).expect("draw");
		let buffer = terminal.backend().buffer();
		(0..height)
			.map(|y| (0..width).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>())
			.collect()
	}

	fn contains(lines: &[String], needle: &str) -> bool {
		lines.iter().any(|line| line.contains(needle))
	}

	#[test]
	fn english_screen_shows_devices_bank_and_hints() {
		let lines = draw("en", &view(), 80, 24);
		assert!(contains(&lines, "sf2_player 0.1.0"));
		assert!(contains(&lines, "MIDI: Arturia KeyStep ●"));
		assert!(contains(&lines, "Audio: Speakers 48000 Hz / 256 frames"));
		assert!(contains(&lines, "SF2: GeneralUser GS (268 presets)"));
		assert!(contains(&lines, "Preset ch1: 000:000 Grand Piano"));
		assert!(contains(&lines, "MIDI monitor"));
		assert!(contains(&lines, "Mixer"));
		assert!(contains(&lines, "xruns 0"));
		assert!(contains(&lines, "F1 help F2 MIDI F3 audio F4 sf2 F7 rec F10 quit"));
	}

	#[test]
	fn russian_screen_uses_the_russian_dictionary() {
		let lines = draw("ru", &view(), 80, 24);
		assert!(contains(&lines, "Аудио: Speakers 48000 Hz / 256 кадров"));
		assert!(contains(&lines, "(268 пресетов)"));
		assert!(contains(&lines, "Монитор MIDI"));
		assert!(contains(&lines, "Микшер"));
		assert!(contains(&lines, "F1 справка"));
		assert!(contains(&lines, "F10 выход"));
	}

	#[test]
	fn missing_devices_and_bank_are_reported() {
		let mut view = view();
		view.midi = MidiStatus::Disconnected("KeyStep".into());
		view.audio = None;
		view.bank = None;
		let lines = draw("en", &view, 80, 24);
		assert!(contains(&lines, "MIDI: KeyStep (disconnected)"));
		assert!(contains(&lines, "Audio: none"));
		assert!(contains(&lines, "SF2: no bank loaded, press F4"));
	}

	#[test]
	fn notice_replaces_the_counters_but_keeps_the_hints() {
		let mut view = view();
		view.notice = Some(Notice::AudioFallback);
		let lines = draw("ru", &view, 80, 24);
		assert!(contains(&lines, "Аудиовыход не найден"));
		assert!(!contains(&lines, "xrun 0"));
		assert!(contains(&lines, "F10 выход"));
	}

	#[test]
	fn dropped_events_appear_only_when_non_zero() {
		let lines = draw("en", &view(), 80, 24);
		assert!(!contains(&lines, "dropped"));
		let mut view = view();
		view.dropped = 3;
		let lines = draw("en", &view, 80, 24);
		assert!(contains(&lines, "dropped 3"));
	}

	#[test]
	fn small_terminal_shows_only_the_size_warning() {
		let lines = draw("en", &view(), 60, 10);
		assert!(contains(&lines, "Terminal too small: need at least 80x24"));
		assert!(!contains(&lines, "Mixer"));
	}

	#[test]
	fn every_line_fits_eighty_columns() {
		for line in draw("ru", &view(), 80, 24) {
			assert_eq!(line.chars().count(), 80);
		}
	}
}
