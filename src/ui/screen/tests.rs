//! Snapshot tests for the main screen and its overlays (ratatui `TestBackend`, 80×24).

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
		transpose: 0,
		omni_channel: None,
		record_dropped: 0,
		held_notes: 0,
		max_voices: 0,
		load_percent: 0,
		levels: Levels::default(),
		mixer: Mixer::new(80),
		reverb: Some(40),
		chorus: None,
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
		"│┌ MIDI monitor ──────────────────────────┐┌ Mixer ───────────────────────────┐│",
		"││                                        ││L ▐░░░░░░░░░░░░▌   -inf dBFS      ││",
		"││                                        ││R ▐░░░░░░░░░░░░▌   -inf dBFS      ││",
		"││                                        ││MASTER ████████░░  80%            ││",
		"││                                        ││FX rev 40 cho off                 ││",
		"││                                        ││ch01 █████░ 100 ch02 █████░ 100   ││",
		"││                                        ││ch03 █████░ 100 ch04 █████░ 100   ││",
		"││                                        ││ch05 █████░ 100 ch06 █████░ 100   ││",
		"││                                        ││ch07 █████░ 100 ch08 █████░ 100   ││",
		"││                                        ││ch09 █████░ 100 ch10 █████░ 100   ││",
		"││                                        ││ch11 █████░ 100 ch12 █████░ 100   ││",
		"││                                        ││ch13 █████░ 100 ch14 █████░ 100   ││",
		"││                                        ││ch15 █████░ 100 ch16 █████░ 100   ││",
		"││                                        ││                                  ││",
		"││                                        ││                                  ││",
		"│└────────────────────────────────────────┘└──────────────────────────────────┘│",
		"│┌ Status ────────────────────────────────────────────────────────────────────┐│",
		"││voices -/- cpu 0% xruns 0 kbd: gate 300ms oct 4 vel 100                     ││",
		"││F1 help F2 MIDI F3 audio F4 sf2 Home/End rec F10 quit                       ││",
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
	assert_eq!(buffer[(43, 3)].fg, Color::Cyan, "mixer border focused");
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
	assert!(contains(&lines, "F1 help F2 MIDI F3 audio F4 sf2 Home/End rec F10 quit"));
}

#[test]
fn effects_dialog_overlay_shows_switches_levels_and_the_highlighted_row() {
	use crate::session::EffectsSettings;
	use crate::ui::effects_dialog::EffectsDialog;
	let mut app = App::new("en");
	app.open_modal(Modal::Effects(EffectsDialog::new(EffectsSettings {
		reverb: true,
		chorus: false,
		reverb_level: 40,
		chorus_level: 7,
	})));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Effects "));
	assert!(contains(&lines, "Reverb   [x]  level  40"), "{lines:#?}");
	assert!(contains(&lines, "Chorus   [ ]  level   7"));
	assert!(contains(&lines, "Space on/off"));
	let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
	terminal.draw(|frame| render(frame, &app, &view())).expect("draw");
	let buffer = terminal.backend().buffer();
	let row = lines.iter().position(|line| line.contains("Reverb   [x]")).expect("reverb row");
	let column = lines[row].find("Reverb").expect("column");
	assert!(
		buffer[(column as u16, row as u16)].modifier.contains(Modifier::REVERSED),
		"current row is highlighted"
	);
	let mut app = App::new("ru");
	app.open_modal(Modal::Effects(EffectsDialog::new(EffectsSettings {
		reverb: false,
		chorus: true,
		reverb_level: 0,
		chorus_level: 127,
	})));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Эффекты "));
	assert!(contains(&lines, "Хорус    [x]  уровень 127"), "{lines:#?}");
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

#[test]
fn help_overlay_lists_the_bindings_in_both_languages() {
	let mut app = App::new("ru");
	app.open_modal(Modal::Help(0));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Справка "));
	assert!(contains(&lines, "F10 / Ctrl+C"));
	assert!(contains(&lines, "Выход"));
	let mut app = App::new("en");
	app.open_modal(Modal::Help(0));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "Shift+Tab"));
	assert!(contains(&lines, "Rescan MIDI"));
	assert!(!contains(&lines, "Ctrl+F"), "last rows need scrolling at 80x24");
	app.open_modal(Modal::Help(100));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "Ctrl+F"), "scroll is clamped to show the last row");
}

#[test]
fn monitor_panel_shows_formatted_entries_and_pause_marker() {
	use crate::midi::{MidiEvent, MonitorEntry};
	let mut app = App::new("en");
	app.monitor.push(MonitorEntry::now(MidiEvent::NoteOn {
		channel: 0,
		key: 60,
		velocity: 92,
	}));
	app.monitor.push(MonitorEntry::now(MidiEvent::ControlChange {
		channel: 0,
		controller: 64,
		value: 127,
	}));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "ch01 NoteOn   C4   vel 92"));
	assert!(contains(&lines, "ch01 CC 64    127"));
	app.monitor.toggle_pause();
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " MIDI monitor [paused] "));
}

#[test]
fn mixer_panel_shows_level_meters_and_clip_mark() {
	let mut loud = view();
	loud.levels = Levels {
		peak_left: 0.5,
		peak_right: 0.25,
		rms_left: 0.25,
		rms_right: 0.125,
	};
	let lines = draw("en", &loud, 80, 24);
	assert!(contains(&lines, "L ▐"), "left meter");
	assert!(contains(&lines, "-6.0 dBFS"), "left peak in dBFS");
	assert!(contains(&lines, "-12.0 dBFS"), "right peak in dBFS");
	assert!(!contains(&lines, "CLIP"));
	let mut app = App::new("ru");
	app.clipping = true;
	let lines = draw_app(&app, &loud, 80, 24);
	assert!(contains(&lines, "CLIP"));
	let silent = draw("ru", &view(), 80, 24);
	assert!(contains(&silent, "−∞ dBFS"), "silence symbol from the dictionary");
}

#[test]
fn mixer_panel_shows_master_and_channel_strips() {
	let mut mixed = view();
	mixed.mixer.toggle_master_mute();
	mixed.mixer.toggle_mute(2);
	mixed.mixer.toggle_solo(0);
	mixed.mixer.adjust_channel(1, -60);
	mixed.channel = 2;
	let lines = draw("en", &mixed, 80, 24);
	assert!(contains(&lines, "MASTER ████████░░  80%  mute"), "{lines:#?}");
	assert!(contains(&lines, "ch01 █████░ solo"));
	assert!(contains(&lines, "ch02 ██░░░░  40"));
	assert!(contains(&lines, "ch03 ────── mute"));
	assert!(contains(&lines, "ch16 █████░ 100"));
	let ru = draw("ru", &mixed, 80, 24);
	assert!(contains(&ru, "МАСТЕР"));
	assert!(contains(&ru, "тихо"));
}

#[test]
fn status_counters_show_held_notes_polyphony_and_load() {
	let lines = draw("en", &view(), 80, 24);
	assert!(
		contains(&lines, "voices -/- cpu 0% xruns 0 kbd: gate 300ms oct 4 vel 100"),
		"no engine installed"
	);
	let mut playing = view();
	playing.held_notes = 7;
	playing.max_voices = 64;
	playing.load_percent = 3;
	playing.xruns = 2;
	let lines = draw("ru", &playing, 80, 24);
	assert!(contains(&lines, "голоса 7/64 cpu 3% xrun 2"));
}

#[test]
fn status_shows_release_mode_in_russian() {
	use crate::ui::text_keyboard::{NoteOffMode, TextKeyboard};
	let app = App::with_keyboard(
		"ru",
		TextKeyboard::new(&crate::config::KeyboardConfig::default(), NoteOffMode::Release),
	);
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "клав: отпускание окт 4 vel 100"));
}

#[test]
fn status_shows_transpose_and_omni_only_when_active() {
	let lines = draw("en", &view(), 80, 24);
	assert!(!contains(&lines, "tr "));
	assert!(!contains(&lines, "omni"));
	let mut shifted = view();
	shifted.transpose = -3;
	shifted.omni_channel = Some(9);
	let lines = draw("en", &shifted, 80, 24);
	assert!(contains(&lines, "tr -3 omni ch10"));
}

#[test]
fn mixer_panel_shows_effect_sends() {
	let lines = draw("en", &view(), 80, 24);
	assert!(contains(&lines, "FX rev 40 cho off"));
	let mut both = view();
	both.chorus = Some(20);
	both.reverb = None;
	let lines = draw("ru", &both, 80, 24);
	assert!(contains(&lines, "FX рев выкл хор 20"));
}

#[test]
fn recording_indicator_and_quit_confirmation() {
	let mut app = App::new("en");
	app.recording = Some(std::time::Duration::from_secs(72));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "● REC 01:12 voices -/-"), "{lines:#?}");
	app.open_modal(Modal::ConfirmQuit);
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Recording in progress "));
	assert!(contains(&lines, "Enter"));
	let mut view_with_notice = view();
	view_with_notice.notice = Some(Notice::RecordSaved("2026-10-20_12-03-41.flac".into()));
	let lines = draw("ru", &view_with_notice, 80, 24);
	assert!(contains(&lines, "Запись сохранена: 2026-10-20_12-03-41.flac"));
}

#[test]
fn ascii_mode_replaces_block_glyphs() {
	let mut app = App::new("en");
	app.glyphs = crate::ui::glyphs::Glyphs::ASCII;
	app.recording = Some(std::time::Duration::from_secs(5));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "MIDI: Arturia KeyStep *"));
	assert!(contains(&lines, "L [------------]"));
	assert!(contains(&lines, "MASTER ########--  80%"));
	assert!(contains(&lines, "ch01 #####- 100"));
	assert!(contains(&lines, "* REC 00:05"));
	for line in &lines {
		assert!(
			!line.contains('█') && !line.contains('░') && !line.contains('●') && !line.contains('▐'),
			"{line}"
		);
	}
}
