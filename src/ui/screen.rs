//! Drawing the main screen (spec §6 mock-up). All text goes through `t!` (ADR 0008).

use cpal::BufferSize;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use rust_i18n::t;

use super::app::Modal;
use super::app::{App, Panel};
use super::dialog::preset_label;
use super::modals;
use crate::midi::MidiStatus;
use crate::session::{Notice, Session};

/// Spec §6: the layout is designed for at least 80×24.
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

/// Everything the screen shows, decoupled from live handles so rendering is testable.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
	/// 1-based channel shown in the header ("Preset ch1").
	pub channel: u8,
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
		let bank = session.bank.as_ref().map(|loaded| BankView {
			name: loaded.bank.name().to_owned(),
			presets: loaded.bank.presets().len(),
			preset_label: session.preset_info(session.current_channel).map(preset_label).unwrap_or_default(),
		});
		Self {
			channel: session.current_channel + 1,
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

	let outer =
		Block::default()
			.borders(Borders::ALL)
			.title(padded(&t!("app.title", locale = locale, version = env!("CARGO_PKG_VERSION"))));
	let inner = outer.inner(area);
	frame.render_widget(outer, area);

	let rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(2), Constraint::Min(5), Constraint::Length(4)])
		.split(inner);
	render_header(frame, rows[0], locale, view);
	render_panels(frame, rows[1], app, locale);
	render_status(frame, rows[2], locale, view);
	match &app.modal {
		Some(Modal::Select(dialog)) => modals::render_dialog(frame, area, dialog, locale, view.channel),
		Some(Modal::Sf2Path(input)) => modals::render_path_dialog(frame, area, input, locale),
		None => {}
	}
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
			label(t!("header.preset", locale = locale, channel = view.channel)),
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
		Notice::AudioReopened(device) => t!("notice.audio_reopened", locale = locale, device = device).into_owned(),
		Notice::AudioError(error) => t!("notice.audio_error", locale = locale, error = error).into_owned(),
		Notice::MidiFallback => t!("notice.midi_fallback", locale = locale).into_owned(),
		Notice::MidiError(error) => t!("notice.midi_error", locale = locale, error = error).into_owned(),
		Notice::Sf2Error(error) => t!("notice.sf2_error", locale = locale, error = error).into_owned(),
	}
}

pub(super) fn label<'a>(text: impl Into<std::borrow::Cow<'a, str>>) -> Span<'a> {
	Span::styled(text.into(), Style::default().add_modifier(Modifier::BOLD))
}

fn panel_block(title: &str, focused: bool) -> Block<'static> {
	let style = if focused {
		Style::default().fg(Color::Cyan)
	} else {
		Style::default()
	};
	Block::default().borders(Borders::ALL).border_style(style).title(padded(title))
}

/// Spec §6 mock-up: titles sit inside the border with a space on each side.
pub(super) fn padded(title: &str) -> String {
	format!(" {title} ")
}

#[cfg(test)]
mod tests {
	use ratatui::Terminal;
	use ratatui::backend::TestBackend;

	use super::*;
	use crate::ui::dialog::Dialog;
	use crate::ui::path_input::PathInput;

	fn view() -> View {
		View {
			channel: 1,
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
		draw_app(&App::new(locale), view, width, height)
	}

	fn draw_app(app: &App, view: &View, width: u16, height: u16) -> Vec<String> {
		let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
		terminal.draw(|frame| render(frame, app, view)).expect("draw");
		let buffer = terminal.backend().buffer();
		(0..height)
			.map(|y| (0..width).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>())
			.collect()
	}

	fn contains(lines: &[String], needle: &str) -> bool {
		lines.iter().any(|line| line.contains(needle))
	}

	/// Golden frame for the spec §6 mock-up: pins the whole 80×24 layout (symbols only; styles are checked separately).
	#[test]
	fn full_frame_matches_the_layout_mockup() {
		let expected: Vec<&str> = vec![
			"┌ sf2_player 0.1.0 ────────────────────────────────────────────────────────────┐",
			"│MIDI: Arturia KeyStep ●   Audio: Speakers 48000 Hz / 256 frames               │",
			"│SF2: GeneralUser GS (268 presets)   Preset ch1: 000:000 Grand Piano           │",
			"│┌ MIDI monitor ───────────────────────┐┌ Mixer ──────────────────────────────┐│",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"││                                     ││                                     ││",
			"│└─────────────────────────────────────┘└─────────────────────────────────────┘│",
			"│┌ Status ────────────────────────────────────────────────────────────────────┐│",
			"││voices -/-  cpu -%  xruns 0                                                 ││",
			"││F1 help F2 MIDI F3 audio F4 sf2 F7 rec F10 quit                             ││",
			"│└────────────────────────────────────────────────────────────────────────────┘│",
			"└──────────────────────────────────────────────────────────────────────────────┘",
		];
		assert_eq!(draw("en", &view(), 80, 24), expected);
	}

	#[test]
	fn focused_panel_border_is_highlighted() {
		let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
		let mut app = App::new("en");
		app.focus = Panel::Mixer;
		terminal.draw(|frame| render(frame, &app, &view())).expect("draw");
		let buffer = terminal.backend().buffer();
		assert_eq!(buffer[(40, 3)].fg, Color::Cyan, "mixer border focused");
		assert_eq!(buffer[(1, 3)].fg, Color::Reset, "monitor border unfocused");
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

	#[test]
	fn dialog_overlay_shows_title_items_marker_and_detail() {
		use crate::audio::AudioDeviceInfo;
		let devices = vec![
			AudioDeviceInfo {
				index: 0,
				name: "Speakers".into(),
				is_default: true,
				default_sample_rate: 48_000,
				buffer_range: Some((128, 2048)),
			},
			AudioDeviceInfo {
				index: 1,
				name: "HDMI Monitor".into(),
				is_default: false,
				default_sample_rate: 44_100,
				buffer_range: None,
			},
		];
		let mut app = App::new("en");
		app.open_dialog(Dialog::audio(&devices, Some("HDMI Monitor"), "en"));
		let lines = draw_app(&app, &view(), 80, 24);
		assert!(contains(&lines, " Audio output "));
		assert!(contains(&lines, "Filter: _"));
		assert!(contains(&lines, "  Speakers"));
		assert!(contains(&lines, "● HDMI Monitor"));
		assert!(
			contains(&lines, "44100 Hz, buffer range unknown"),
			"detail of the highlighted (current) item"
		);
		assert!(contains(&lines, "Enter apply"));
		assert!(contains(&lines, "MIDI monitor"), "main screen still visible around the modal");
	}

	#[test]
	fn empty_dialog_says_so_in_russian() {
		let mut app = App::new("ru");
		app.open_dialog(Dialog::midi(&[], None));
		let lines = draw_app(&app, &view(), 80, 24);
		assert!(contains(&lines, " MIDI-вход "));
		assert!(contains(&lines, "выбирать нечего"));
	}

	#[test]
	fn sf2_path_dialog_shows_field_error_recent_and_cursor() {
		use std::path::PathBuf;
		let mut app = App::new("en");
		let mut input = PathInput::new(
			"assets/sin_wave.sf2",
			vec![PathBuf::from("C:/banks/GeneralUser.sf2"), PathBuf::from("old.sf2")],
		);
		input.set_error("Cargo.toml is not a valid SoundFont 2 file".into());
		app.open_modal(Modal::Sf2Path(input));
		let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
		terminal.draw(|frame| render(frame, &app, &view())).expect("draw");
		let buffer = terminal.backend().buffer();
		let lines: Vec<String> = (0..24)
			.map(|y| (0..80).map(|x| buffer[(x, y)].symbol().to_owned()).collect())
			.collect();
		assert!(contains(&lines, " SoundFont file "));
		assert!(contains(&lines, "Path: assets/sin_wave.sf2"));
		assert!(contains(&lines, "not a valid SoundFont 2 file"));
		assert!(contains(&lines, "  C:/banks/GeneralUser.sf2"));
		assert!(contains(&lines, "Tab complete"));
		let (cursor_x, cursor_y) = {
			let mut backend_terminal = terminal;
			let position = backend_terminal.get_cursor_position().expect("cursor");
			(position.x, position.y)
		};
		let field_row = lines.iter().position(|line| line.contains("Path: assets")).unwrap();
		assert_eq!(usize::from(cursor_y), field_row);
		let row: Vec<char> = lines[field_row].chars().collect();
		let needle: Vec<char> = "assets/sin_wave.sf2".chars().collect();
		let path_col = row
			.windows(needle.len())
			.position(|window| window == needle.as_slice())
			.expect("path in row");
		assert_eq!(
			usize::from(cursor_x),
			path_col + "assets/sin_wave.sf2".chars().count(),
			"cursor after the last character"
		);
	}
}
