//! Snapshot tests for the main screen and its overlays (ratatui `TestBackend`, 80×24).

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use super::*;
use crate::ui::dialog::Dialog;
use crate::ui::path_input::PathInput;

fn view() -> View {
	View {
		midi: MidiStatus::Connected("Arturia KeyStep".into()),
		audio: Some(AudioView {
			device: "Speakers".into(),
			sample_rate: 48_000,
			buffer: BufferSize::Fixed(256),
		}),
		bank: Some(BankView {
			file_name: "GeneralUser GS.sf2".into(),
			preset_label: "000:000 Grand Piano".into(),
		}),
		reverb: Some(40),
		chorus: None,
		record_dir: "D:/Music/sf2_player".into(),
		record_format: RecordFormat::Flac,
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
		notice: None,
	}
}

fn draw(locale: &'static str, view: &View, width: u16, height: u16) -> Vec<String> {
	draw_app(&App::new(locale), view, width, height)
}

fn draw_app(app: &App, view: &View, width: u16, height: u16) -> Vec<String> {
	lines_of(&render_to_buffer(app, view, width, height))
}

fn render_to_buffer(app: &App, view: &View, width: u16, height: u16) -> Buffer {
	let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
	terminal.draw(|frame| render(frame, app, view)).expect("draw");
	terminal.backend().buffer().clone()
}

fn lines_of(buffer: &Buffer) -> Vec<String> {
	let area = buffer.area;
	(0..area.height)
		.map(|y| (0..area.width).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>())
		.collect()
}

fn contains(lines: &[String], needle: &str) -> bool {
	lines.iter().any(|line| line.contains(needle))
}

/// Row index of the first line containing `needle`.
fn row_of(lines: &[String], needle: &str) -> usize {
	lines
		.iter()
		.position(|line| line.contains(needle))
		.unwrap_or_else(|| panic!("{needle:?} not on screen:\n{}", lines.join("\n")))
}

/// Column (in cells) of `needle` within `line`.
fn column_of(line: &str, needle: &str) -> usize {
	let byte = line.find(needle).unwrap_or_else(|| panic!("{needle:?} not in {line:?}"));
	line[..byte].chars().count()
}

/// Foreground colour of the first cell of `needle`.
fn colour_of(buffer: &Buffer, needle: &str) -> Color {
	let lines = lines_of(buffer);
	let row = row_of(&lines, needle);
	let column = column_of(&lines[row], needle);
	buffer[(column as u16, row as u16)].fg
}

/// Golden frame for the spec v1.1 §2 mock-up: pins the whole 80×24 layout (symbols only; styles are checked separately).
#[test]
fn full_frame_matches_the_layout_mockup() {
	let expected: Vec<&str> = vec![
		"┌────────────────────────────── sf2_player 0.1.0 ──────────────────────────────┐",
		"│[F1] Help                                                                     │",
		"│[F2] MIDI:      Arturia KeyStep                                               │",
		"│[F3] Audio:     Speakers 48000 Hz / 256 frames  OK                            │",
		"│[F4] SF2:       GeneralUser GS.sf2                                            │",
		"│[F5] Preset:    000:000 Grand Piano                                           │",
		"│[F6] Effects:   rev 40  cho off                                               │",
		"│[F7] Recording: D:/Music/sf2_player  FLAC                                     │",
		"│     [Home] start recording   [End] stop                                      │",
		"│                                                                              │",
		"│                                                                              │",
		"│Mixer                                                                         │",
		"│  L  ▐░░░░░░░░░░░░▌    -inf dBFS                                              │",
		"│  R  ▐░░░░░░░░░░░░▌    -inf dBFS                                              │",
		"│  MASTER ████████░░   80%                                                     │",
		"│                                                                              │",
		"│                                                                              │",
		"│                                                                              │",
		"│                                                                              │",
		"│                                                                              │",
		"│                                                                              │",
		"│                                                                              │",
		"│                                                                              │",
		"└ voices   -/-   cpu   0% xrun     0 kbd: gate 300ms  oct 4 vel 100 ───────────┘",
	];
	assert_eq!(draw("en", &view(), 80, 24), expected);
}

#[test]
fn russian_screen_uses_the_russian_dictionary() {
	let lines = draw("ru", &view(), 80, 24);
	assert!(contains(&lines, "[F1] Справка"));
	assert!(contains(&lines, "[F3] Аудио:   Speakers 48000 Hz / 256 кадров  OK"));
	assert!(contains(&lines, "[F5] Пресет:  000:000 Grand Piano"));
	assert!(contains(&lines, "[F6] Эффекты: рев 40  хор выкл"));
	assert!(contains(&lines, "[Home] начать запись   [End] остановить"));
	assert!(contains(&lines, "Микшер"));
	assert!(contains(&lines, "МАСТЕР"));
	assert!(contains(&lines, "−∞ dBFS"), "silence symbol from the dictionary");
	assert!(
		contains(&lines, "└ голоса   -/-   cpu   0% xrun     0 клав: gate 300мс окт 4 vel 100 "),
		"{lines:#?}"
	);
}

#[test]
fn missing_devices_and_bank_are_red_and_audio_ok_is_green() {
	let app = App::new("en");
	let ok = render_to_buffer(&app, &view(), 80, 24);
	assert_eq!(colour_of(&ok, "OK"), Color::Green);
	assert_eq!(colour_of(&ok, "Arturia KeyStep"), Color::Reset, "a connected port is not an error");

	let mut missing = view();
	missing.midi = MidiStatus::NoPort;
	missing.audio = None;
	missing.bank = None;
	let buffer = render_to_buffer(&app, &missing, 80, 24);
	let lines = lines_of(&buffer);
	assert!(contains(&lines, "[F2] MIDI:      no device"), "{lines:#?}");
	assert!(contains(&lines, "[F3] Audio:     no device"));
	assert!(contains(&lines, "[F4] SF2:       not set, press F4"));
	assert!(contains(&lines, "[F5] Preset:    no bank"));
	assert_eq!(colour_of(&buffer, "no device"), Color::Red);
	assert_eq!(colour_of(&buffer, "not set, press F4"), Color::Red);
	assert_eq!(colour_of(&buffer, "no bank"), Color::DarkGray);
	assert!(!contains(&lines, "OK"), "no green OK without a stream");

	let mut disconnected = view();
	disconnected.midi = MidiStatus::Disconnected("KeyStep".into());
	let buffer = render_to_buffer(&app, &disconnected, 80, 24);
	assert!(contains(&lines_of(&buffer), "[F2] MIDI:      KeyStep (disconnected)"));
	assert_eq!(colour_of(&buffer, "KeyStep (disconnected)"), Color::Red);
}

#[test]
fn notices_land_in_the_line_they_belong_to_or_in_the_message_line() {
	let app = App::new("en");
	// (notice, expected row, the whole line fragment, the coloured part of it, its colour)
	let cases: [(Notice, usize, &str, &str, Color); 6] = [
		(
			Notice::MidiError("port busy".into()),
			2,
			"Arturia KeyStep  port busy",
			"port busy",
			Color::Red,
		),
		(
			Notice::AudioError("stream lost".into()),
			3,
			"256 frames  stream lost",
			"stream lost",
			Color::Red,
		),
		(
			Notice::Sf2Error("not a SoundFont".into()),
			4,
			"GeneralUser GS.sf2  not a SoundFont",
			"not a SoundFont",
			Color::Red,
		),
		(
			Notice::RecordError("disk full".into()),
			7,
			"FLAC  disk full",
			"disk full",
			Color::Red,
		),
		(
			Notice::RecordSaved("take.flac".into()),
			9,
			"Recording saved: take.flac",
			"Recording saved",
			Color::Yellow,
		),
		(
			Notice::ConfigError("read-only".into()),
			9,
			"Config not saved: read-only",
			"Config not saved",
			Color::Yellow,
		),
	];
	for (notice, row, text, coloured, colour) in cases {
		let mut with_notice = view();
		with_notice.notice = Some(notice.clone());
		let buffer = render_to_buffer(&app, &with_notice, 80, 24);
		let lines = lines_of(&buffer);
		assert_eq!(row_of(&lines, text), row, "{notice:?}\n{lines:#?}");
		assert_eq!(colour_of(&buffer, coloured), colour, "{notice:?}");
	}
	let mut no_audio = view();
	no_audio.audio = None;
	no_audio.notice = Some(Notice::NoAudioDevice("no output".into()));
	let lines = draw("en", &no_audio, 80, 24);
	assert!(contains(&lines, "[F3] Audio:     no output"), "the error replaces the device text");
	assert!(!contains(&lines, "OK"));
	let mut fallback = view();
	fallback.notice = Some(Notice::AudioFallback);
	let lines = draw("ru", &fallback, 80, 24);
	assert_eq!(row_of(&lines, "Аудиовыход не найден"), 9);
	assert!(contains(&lines, "OK"), "a fallback notice is not an error of the line");
}

#[test]
fn status_fields_keep_their_columns_whatever_the_values() {
	let app = App::new("en");
	let reference = draw_app(&app, &view(), 80, 24);
	let status = &reference[23];
	let columns = |line: &str| ["cpu", "xrun", "kbd:", "oct", "vel"].map(|token| column_of(line, token));
	let mut loud = view();
	loud.held_notes = 999;
	loud.max_voices = 256;
	loud.load_percent = 100;
	loud.xruns = 99_999;
	let busy = draw_app(&app, &loud, 80, 24);
	assert!(busy[23].contains("voices 999/256 cpu 100% xrun 99999 "), "{}", busy[23]);
	assert_eq!(columns(&busy[23]), columns(status));
	let mut overflow = view();
	overflow.xruns = 100_000;
	let line = &draw_app(&app, &overflow, 80, 24)[23];
	assert!(line.contains("xrun 9999+ "), "{line}");
	assert_eq!(columns(line), columns(status));

	use crate::ui::text_keyboard::{NoteOffMode, TextKeyboard};
	let release = App::with_keyboard(
		"ru",
		TextKeyboard::new(&crate::config::KeyboardConfig::default(), NoteOffMode::Release),
	);
	let gate = App::new("ru");
	let release_line = &draw_app(&release, &view(), 80, 24)[23];
	let gate_line = &draw_app(&gate, &view(), 80, 24)[23];
	assert!(release_line.contains("клав: отпускание"), "{release_line}");
	assert_eq!(column_of(release_line, "окт"), column_of(gate_line, "окт"));
	assert_eq!(column_of(release_line, "vel"), column_of(gate_line, "vel"));
}

#[test]
fn optional_status_fields_sit_on_the_right_and_never_push_the_fixed_ones() {
	let lines = draw("en", &view(), 80, 24);
	assert!(!lines[23].contains("tr ") && !lines[23].contains("omni") && !lines[23].contains("dropped"));
	let mut shifted = view();
	shifted.transpose = -3;
	shifted.omni_channel = Some(9);
	shifted.dropped = 3;
	let wide = &draw("en", &shifted, 100, 24)[23];
	assert!(wide.ends_with("─ tr -3 omni ch10 dropped 3 ┘"), "{wide}");
	assert!(
		wide.starts_with("└ voices   -/-   cpu   0% xrun     0 kbd: gate 300ms  oct 4 vel 100 ─"),
		"{wide}"
	);
	let narrow = &draw("en", &shifted, 80, 24)[23];
	assert!(
		narrow.starts_with("└ voices   -/-   cpu   0% xrun     0 kbd: gate 300ms  oct 4 vel 100 "),
		"fixed fields win at 80 columns: {narrow}"
	);
	assert!(!narrow.contains("omni"), "no room for all three at 80 columns: {narrow}");
	assert_eq!(column_of(narrow, "cpu"), column_of(&lines[23], "cpu"));
	let mut transposed = view();
	transposed.transpose = 12;
	let line = &draw("en", &transposed, 80, 24)[23];
	assert!(
		line.ends_with("─ tr +12 ┘"),
		"a short optional field still fits at 80 columns: {line}"
	);
}

#[test]
fn recording_indicator_blinks_in_the_recording_line_and_quit_asks_for_confirmation() {
	let mut app = App::new("en");
	app.recording = Some(std::time::Duration::from_millis(72_000));
	let lit = draw_app(&app, &view(), 80, 24);
	assert_eq!(row_of(&lit, "FLAC  ● REC 01:12"), 7, "{lit:#?}");
	app.recording = Some(std::time::Duration::from_millis(72_600));
	let dark = draw_app(&app, &view(), 80, 24);
	assert_eq!(row_of(&dark, "FLAC    REC 01:12"), 7, "{dark:#?}");
	assert_eq!(record_blink(std::time::Duration::from_millis(499), &app.glyphs), "●");
	assert_eq!(record_blink(std::time::Duration::from_millis(500), &app.glyphs), " ");
	assert_eq!(
		record_blink(std::time::Duration::from_millis(1_000), &app.glyphs),
		"●",
		"period is one second"
	);
	assert_eq!(colour_of(&render_to_buffer(&app, &view(), 80, 24), "REC 01:12"), Color::Red);
	app.open_modal(Modal::ConfirmQuit);
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Recording in progress "));
	assert!(contains(&lines, "Enter"));
	let mut ru = view();
	ru.record_format = RecordFormat::OggVorbis;
	let app = App::new("ru");
	let lines = draw_app(&app, &ru, 80, 24);
	assert!(contains(&lines, "[F7] Запись:  D:/Music/sf2_player  Ogg Vorbis"), "{lines:#?}");
}

#[test]
fn mixer_shows_meters_clip_mark_and_master_mute() {
	let mut loud = view();
	loud.levels = Levels {
		peak_left: 0.5,
		peak_right: 0.25,
		rms_left: 0.25,
		rms_right: 0.125,
	};
	loud.mixer.toggle_master_mute();
	loud.mixer.adjust_master(-30);
	let lines = draw("en", &loud, 80, 24);
	assert!(contains(&lines, "  L  ▐"), "left meter");
	assert!(contains(&lines, "-6.0 dBFS"), "left peak in dBFS");
	assert!(contains(&lines, "-12.0 dBFS"), "right peak in dBFS");
	assert!(!contains(&lines, "CLIP"));
	assert!(contains(&lines, "  MASTER █████░░░░░   50%  mute"), "{lines:#?}");
	let mut app = App::new("ru");
	app.clipping = true;
	let lines = draw_app(&app, &loud, 80, 24);
	assert!(contains(&lines, "dBFS  CLIP"));
	assert!(contains(&lines, "тихо"));
	assert!(!contains(&lines, "ch01"), "channel strips are gone (ADR 0010)");
}

#[test]
fn small_terminal_shows_only_the_size_warning() {
	let lines = draw("en", &view(), 60, 10);
	assert!(contains(&lines, "Terminal too small: need at least 80x24"));
	assert!(!contains(&lines, "Mixer"));
}

#[test]
fn every_line_fits_eighty_columns() {
	let mut long = view();
	long.record_dir = "C:/Users/Somebody With A Long Name/Music/sf2_player/recordings/2026".into();
	long.transpose = 12;
	long.omni_channel = Some(15);
	for line in draw("ru", &long, 80, 24) {
		assert_eq!(line.chars().count(), 80, "{line}");
	}
}

#[test]
fn dialog_overlay_shows_title_items_marker_and_detail() {
	use crate::audio::{AudioDeviceInfo, Backend};
	let devices = vec![
		AudioDeviceInfo {
			index: 0,
			name: "Speakers".into(),
			backend: Backend::Default,
			is_default: true,
			default_sample_rate: 48_000,
			buffer_range: Some((128, 2048)),
		},
		AudioDeviceInfo {
			index: 1,
			name: "HDMI Monitor".into(),
			backend: Backend::Default,
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
	assert!(contains(&lines, "[F1] Help"), "main screen still visible around the modal");
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
	let lines = lines_of(terminal.backend().buffer());
	assert!(contains(&lines, " SoundFont file "));
	assert!(contains(&lines, "Path: assets/sin_wave.sf2"));
	assert!(contains(&lines, "not a valid SoundFont 2 file"));
	assert!(contains(&lines, "  C:/banks/GeneralUser.sf2"));
	assert!(contains(&lines, "Tab complete"));
	let position = terminal.get_cursor_position().expect("cursor");
	let field_row = row_of(&lines, "Path: assets");
	assert_eq!(usize::from(position.y), field_row);
	assert_eq!(
		usize::from(position.x),
		column_of(&lines[field_row], "assets/sin_wave.sf2") + "assets/sin_wave.sf2".chars().count(),
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
	assert!(!contains(&lines, "Tab "), "panel focus is gone (ADR 0010)");
	let mut app = App::new("en");
	app.open_modal(Modal::Help(0));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "Home"));
	assert!(contains(&lines, "Start recording"));
	assert!(contains(&lines, "Rescan MIDI"));
	assert!(!contains(&lines, "S D G H J L ;"), "last rows need scrolling at 80x24");
	app.open_modal(Modal::Help(100));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "S D G H J L ;"), "scroll is clamped to show the last row");
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
	let buffer = render_to_buffer(&app, &view(), 80, 24);
	let lines = lines_of(&buffer);
	assert!(contains(&lines, " Effects "));
	assert!(contains(&lines, "Reverb   [x]  level  40"), "{lines:#?}");
	assert!(contains(&lines, "Chorus   [ ]  level   7"));
	assert!(contains(&lines, "Space on/off"));
	let row = row_of(&lines, "Reverb   [x]");
	let column = column_of(&lines[row], "Reverb");
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
fn record_dialog_overlay_shows_folder_format_radio_and_description() {
	use crate::ui::record_dialog::RecordDialog;
	let mut app = App::new("en");
	app.open_modal(Modal::Record(RecordDialog::new(None, RecordFormat::Flac, 0.5)));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Recording "));
	assert!(contains(&lines, "Folder: (empty: Music folder/sf2_player)"), "{lines:#?}");
	assert!(contains(&lines, "Format: (●) FLAC   ( ) Ogg Vorbis"));
	assert!(contains(&lines, "lossless, about 3–5 MB/min"));
	assert!(contains(&lines, "Tab complete"));
	let mut dialog = RecordDialog::new(Some(std::path::Path::new("D:/takes")), RecordFormat::OggVorbis, 0.5);
	dialog.handle_key(
		ratatui::crossterm::event::KeyEvent::new(
			ratatui::crossterm::event::KeyCode::Down,
			ratatui::crossterm::event::KeyModifiers::NONE,
		),
		|_| Vec::new(),
	);
	let mut app = App::new("ru");
	app.open_modal(Modal::Record(dialog));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, " Запись "));
	assert!(contains(&lines, "Каталог: D:/takes"));
	assert!(contains(&lines, "Формат: ( ) FLAC   (●) Ogg Vorbis"));
	assert!(contains(&lines, "качество 0.5"));
}

#[test]
fn ascii_mode_replaces_block_glyphs() {
	let mut app = App::new("en");
	app.glyphs = crate::ui::glyphs::Glyphs::ASCII;
	app.recording = Some(std::time::Duration::from_secs(5));
	let lines = draw_app(&app, &view(), 80, 24);
	assert!(contains(&lines, "  L  [------------]"));
	assert!(contains(&lines, "  MASTER ########--   80%"));
	assert!(contains(&lines, "FLAC  * REC 00:05"));
	for line in &lines {
		assert!(
			!line.contains('█') && !line.contains('░') && !line.contains('●') && !line.contains('▐'),
			"{line}"
		);
	}
}
