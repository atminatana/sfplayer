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
use super::meter::{dbfs_text, level_bar};
use super::modals;
use super::text_keyboard::NoteOffMode;
use crate::audio::mixer::CHANNELS;
use crate::audio::{Levels, Mixer};
use crate::midi::MidiStatus;
use crate::session::{Notice, Session};

/// Spec §6: the layout is designed for at least 80×24.
pub const MIN_WIDTH: u16 = 80;
/// Monitor panel width (borders included): one "HH:MM:SS.mmm  ch01 NoteOn   C4   vel 92" line fits inside.
const MONITOR_WIDTH: u16 = 42;
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
	pub transpose: i8,
	pub omni_channel: Option<u8>,
	pub record_dropped: u32,
	pub held_notes: u32,
	pub max_voices: u32,
	pub load_percent: u32,
	pub levels: Levels,
	pub mixer: Mixer,
	/// Reverb / chorus send levels; `None` = off (F-13).
	pub reverb: Option<u8>,
	pub chorus: Option<u8>,
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
			transpose: session.transpose,
			omni_channel: session.omni_channel(),
			record_dropped: session.shared.record_dropped(),
			held_notes: session.shared.held_notes(),
			max_voices: session.shared.max_voices(),
			load_percent: session.shared.load_percent(),
			levels: session.shared.levels(),
			mixer: session.mixer.clone(),
			reverb: session.reverb_level(),
			chorus: session.chorus_level(),
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
	render_panels(frame, rows[1], app, locale, view);
	render_status(frame, rows[2], app, locale, view);
	match &app.modal {
		Some(Modal::Select(dialog)) => modals::render_dialog(frame, area, dialog, locale, view.channel),
		Some(Modal::Sf2Path(input)) => modals::render_path_dialog(frame, area, input, locale),
		Some(Modal::Help(scroll)) => modals::render_help(frame, area, *scroll, locale),
		Some(Modal::ConfirmQuit) => modals::render_confirm_quit(frame, area, locale),
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

fn render_panels(frame: &mut Frame, area: Rect, app: &App, locale: &str, view: &View) {
	let columns = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Length(MONITOR_WIDTH), Constraint::Min(20)])
		.split(area);
	render_monitor(frame, columns[0], app, locale);
	render_mixer(frame, columns[1], app, locale, view);
}

/// Width of the level bars in the mixer panel.
const METER_BAR_WIDTH: usize = 12;

/// Mixer panel: output meters (F-09) at the top; master and channel strips follow in step 4.3.
fn render_mixer(frame: &mut Frame, area: Rect, app: &App, locale: &str, view: &View) {
	let block = panel_block(t!("panel.mixer", locale = locale).as_ref(), app.focus == Panel::Mixer);
	let inner = block.inner(area);
	frame.render_widget(block, area);
	let silence = t!("meter.silence", locale = locale);
	let clip = if app.clipping {
		format!("  {}", t!("meter.clip", locale = locale))
	} else {
		String::new()
	};
	let meter_line = |name: &str, peak: f32, rms: f32| {
		let mut spans = vec![
			Span::raw(format!("{name} ▐")),
			Span::raw(level_bar(rms, METER_BAR_WIDTH)),
			Span::raw(format!("▌ {}", dbfs_text(peak, &silence))),
		];
		if !clip.is_empty() {
			spans.push(Span::styled(
				clip.clone(),
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
			));
		}
		Line::from(spans)
	};
	let mut lines = vec![
		meter_line("L", view.levels.peak_left, view.levels.rms_left),
		meter_line("R", view.levels.peak_right, view.levels.rms_right),
	];
	lines.push(master_line(view, locale));
	lines.push(effects_line(view, locale));
	lines.extend(channel_lines(view, locale));
	frame.render_widget(Paragraph::new(lines), inner);
}

/// "MASTER ████████░░  80%" plus a mute mark.
fn master_line(view: &View, locale: &str) -> Line<'static> {
	let mixer = &view.mixer;
	let mut spans = vec![
		label(t!("mixer.master", locale = locale).into_owned()),
		Span::raw(format!(" {} {:>3}%", percent_bar(mixer.master, 10), mixer.master)),
	];
	if mixer.master_muted {
		spans.push(Span::styled(
			format!("  {}", t!("mixer.mute", locale = locale)),
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
		));
	}
	Line::from(spans)
}

/// "FX rev 40 cho off" — reverb and chorus send levels or "off" (F-13).
fn effects_line(view: &View, locale: &str) -> Line<'static> {
	let off = t!("mixer.off", locale = locale);
	let level = |value: Option<u8>| value.map_or_else(|| off.to_string(), |level| level.to_string());
	Line::from(vec![
		label(t!("mixer.fx", locale = locale).into_owned()),
		Span::raw(format!(
			" {} {} {} {}",
			t!("mixer.reverb", locale = locale),
			level(view.reverb),
			t!("mixer.chorus", locale = locale),
			level(view.chorus)
		)),
	])
}

/// Two channel strips per line: "ch01 ████░░ 100  ch02 ───── mute"; the current channel is highlighted.
fn channel_lines(view: &View, locale: &str) -> Vec<Line<'static>> {
	let mixer = &view.mixer;
	let current = usize::from(view.channel.saturating_sub(1));
	(0..CHANNELS)
		.step_by(2)
		.map(|first| {
			let mut spans = Vec::new();
			for channel in [first, first + 1] {
				let strip = mixer.channels[channel];
				let text = if strip.muted {
					format!("ch{:02} ────── {}", channel + 1, t!("mixer.mute", locale = locale))
				} else {
					let mark = if strip.solo {
						t!("mixer.solo", locale = locale).into_owned()
					} else {
						format!("{:>3}", strip.volume)
					};
					format!("ch{:02} {} {mark}", channel + 1, percent_bar(strip.volume, 6))
				};
				let style = if channel == current {
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
				} else {
					Style::default()
				};
				spans.push(Span::styled(format!("{text:<16}"), style));
			}
			Line::from(spans)
		})
		.collect()
}

/// Bar for a 0..=max value; the master uses percent (max 100), channels CC 7 (max 127).
fn percent_bar(value: u8, width: usize) -> String {
	let max = if width == 10 { 100.0 } else { 127.0 };
	let filled = ((f32::from(value) / max) * width as f32).round() as usize;
	let filled = filled.min(width);
	format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// MIDI monitor panel (F-08): newest at the bottom unless scrolled; title shows the pause state.
fn render_monitor(frame: &mut Frame, area: Rect, app: &App, locale: &str) {
	let mut title = t!("panel.monitor", locale = locale).into_owned();
	if app.monitor.is_paused() {
		title.push_str(&format!(" {}", t!("panel.monitor_paused", locale = locale)));
	}
	if app.monitor.scroll() > 0 {
		title.push_str(&format!(
			" {}",
			t!("panel.monitor_scrolled", locale = locale, rows = app.monitor.scroll())
		));
	}
	let block = panel_block(&title, app.focus == Panel::Monitor);
	let inner = block.inner(area);
	frame.render_widget(block, area);
	let lines: Vec<Line> = app
		.monitor
		.window(usize::from(inner.height))
		.map(|entry| Line::from(entry.format()))
		.collect();
	frame.render_widget(Paragraph::new(lines), inner);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, locale: &str, view: &View) {
	let block = panel_block(t!("panel.status", locale = locale).as_ref(), false);
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let rec = app.recording.map(|elapsed| {
		Span::styled(
			format!(
				"{} ",
				t!(
					"status.rec",
					locale = locale,
					time = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
				)
			),
			Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
		)
	});
	let first = match &view.notice {
		Some(notice) => Line::from(Span::styled(notice_text(notice, locale), Style::default().fg(Color::Yellow))),
		None => {
			let mut counters = vec![
				t!("status.voices", locale = locale, voices = voices_text(view)).into_owned(),
				t!("status.cpu", locale = locale, percent = view.load_percent).into_owned(),
				t!("status.xruns", locale = locale, count = view.xruns).into_owned(),
				keyboard_text(app, locale),
			];
			if view.transpose != 0 {
				counters.push(t!("status.transpose", locale = locale, value = format!("{:+}", view.transpose)).into_owned());
			}
			if let Some(channel) = view.omni_channel {
				counters.push(t!("status.omni", locale = locale, channel = channel + 1).into_owned());
			}
			if view.dropped > 0 {
				counters.push(t!("status.dropped", locale = locale, count = view.dropped).into_owned());
			}
			if view.record_dropped > 0 {
				counters.push(t!("status.rec_dropped", locale = locale, count = view.record_dropped).into_owned());
			}
			let mut spans = Vec::new();
			if let Some(rec) = rec {
				spans.push(rec);
			}
			spans.push(Span::raw(counters.join(" ")));
			Line::from(spans)
		}
	};
	let hints = ["hint.help", "hint.midi", "hint.audio", "hint.sf2", "hint.record", "hint.quit"]
		.iter()
		.map(|key| t!(*key, locale = locale).into_owned())
		.collect::<Vec<_>>()
		.join(" ");
	frame.render_widget(Paragraph::new(vec![first, Line::from(hints)]), inner);
}

/// "kbd: gate 300ms  oct 4 vel 100" — note-off mode, octave and velocity of the text keyboard (F-16).
fn keyboard_text(app: &App, locale: &str) -> String {
	let mode = match app.keyboard.mode() {
		NoteOffMode::Release => t!("status.kbd_release", locale = locale).into_owned(),
		NoteOffMode::Gate(gate) => t!("status.kbd_gate", locale = locale, ms = gate.as_millis()).into_owned(),
	};
	format!(
		"{mode} {}",
		t!(
			"status.octave",
			locale = locale,
			octave = app.keyboard.octave(),
			velocity = app.keyboard.velocity()
		)
	)
}

/// "7/64" while an engine is installed, "-/-" otherwise.
fn voices_text(view: &View) -> String {
	if view.max_voices == 0 {
		"-/-".to_owned()
	} else {
		format!("{}/{}", view.held_notes, view.max_voices)
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
mod tests;
