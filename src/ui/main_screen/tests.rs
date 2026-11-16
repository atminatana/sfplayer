//! Snapshot tests of the window main screen: text lines and cell colours of a `ScreenBuffer`, no window.

use std::time::Duration;

use gterm::cell::{CellFlags, Color};
use gterm_widgets::{MouseKind, MouseMsg};
use winit::event::MouseButton;
use winit::keyboard::ModifiersState;

use super::output::{fitted_end, record_dot};
use super::*;
use crate::audio::{Levels, Mixer};
use crate::config::{AdsrField, BuiltinConfig, RecordFormat};
use crate::midi::MidiStatus;
use crate::midi::player::Transport;
use crate::session::MidiFile;
use crate::session::Notice;
use crate::synth::AdsrPreset;
use crate::ui::buttons::Touch;
use crate::ui::chord::{KeyCode, KeyEvent, KeyModifiers};
use crate::ui::view::{AudioView, BankView, FileView, Size, TakeView};

fn view() -> View {
	View {
		midi: MidiStatus::Connected("Arturia KeyStep 37".into()),
		audio: Some(AudioView {
			device: "Speakers".into(),
			sample_rate: 48_000,
			buffer: Some(256),
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
		held_notes: 7,
		max_voices: 64,
		load_percent: 3,
		levels: Levels::default(),
		mixer: Mixer::new(80),
		notice: None,
		keyboard_notes: 0,
		midi_notes: 0,
		engine: EngineKind::Sf2,
		builtin: BuiltinConfig::default(),
		file: FileView {
			file: MidiFile::None,
			transport: Transport::Stopped,
			position_tenths: 0,
			duration_tenths: 0,
		},
		take: TakeView::None,
	}
}

fn screen(cols: usize, rows: usize) -> ScreenBuffer {
	ScreenBuffer::with_default_colors(cols, rows, TEXT, BACKGROUND)
}

/// The screen as text, one string per row; an empty cell is a space, trailing spaces are dropped.
fn lines_of(buf: &ScreenBuffer) -> Vec<String> {
	(0..buf.rows())
		.map(|row| {
			let line: String = (0..buf.cols())
				.map(|col| match buf.cell(col, row).ch {
					'\0' => ' ',
					ch => ch,
				})
				.collect();
			line.trim_end().to_owned()
		})
		.collect()
}

fn rendered(app: &App, view: &View) -> ScreenBuffer {
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, app, view, &mut Buttons::new());
	buf
}

fn rendered_ru(view: &View) -> ScreenBuffer {
	rendered(&App::new("ru"), view)
}

/// Row and column (in cells) of the first occurrence of `text`.
fn find(buf: &ScreenBuffer, text: &str) -> (usize, usize) {
	for (row, line) in lines_of(buf).iter().enumerate() {
		if let Some(byte) = line.find(text) {
			return (row, line[..byte].chars().count());
		}
	}
	panic!("{text:?} is not on the screen:\n{}", lines_of(buf).join("\n"));
}

fn is_shown(buf: &ScreenBuffer, text: &str) -> bool {
	lines_of(buf).iter().any(|line| line.contains(text))
}

/// Foreground colour of the first cell of `text`.
fn colour_of(buf: &ScreenBuffer, text: &str) -> Color {
	let (row, col) = find(buf, text);
	buf.cell(col, row).fg
}

/// Foreground colours of the `cells` cells that follow the label column in the row labelled `label`.
fn meter_colours(buf: &ScreenBuffer, label: &str, cells: usize) -> Vec<Color> {
	let (row, _) = find(buf, label);
	(0..cells).map(|cell| buf.cell(usize::from(VALUE_X) + cell, row).fg).collect()
}

#[test]
fn full_frame_matches_the_layout_mockup() {
	let expected = [
		"─────────────────────────────────────── sf2_player 0.1.0 ───────────────────────────────────────",
		"",
		"ИСТОЧНИКИ MIDI ─────────────────────────────────────────────────────────────────────────────────",
		" Клавиатура        —                                                 окт 4 vel 100",
		"                                │ ██ ██  │ ██ ██ ██  │ ██ ██  │",
		"                                │  │  │  │  │  │  │  │  │  │  │",
		" MIDI-вход         [MIDI-вход]  Arturia KeyStep 37  открыто",
		"   → ноты          —",
		" MIDI-файл         [MIDI-файл]  файл не выбран  [Старт] [Пауза] [Стоп]",
		"",
		"ОБРАБОТЧИК  [Synth] [Sound Font] ───────────────────────────────────────────────────────────────",
		" Файл Sound Font   [Файл Sound Font]  GeneralUser GS.sf2  OK",
		" Пресет            [Пресет]  000:000 Grand Piano",
		" Эффекты           [Эффекты]  рев 40  хор выкл",
		"",
		"ВЫХОД ──────────────────────────────────────────────────────────────────────────────────────────",
		" Аудио выход       [Аудио выход]  Speakers  48000 Hz / 256 кадров  OK",
		" L                 ░░░░░░░░░░░░░░░░░░░░░░░░░░      −∞ dBFS",
		" R                 ░░░░░░░░░░░░░░░░░░░░░░░░░░      −∞ dBFS",
		" Мастер            [−] ████████████████░░░░  80% [+]",
		"",
		"АУДИО-ФАЙЛ ─────────────────────────────────────────────────────────────────────────────────────",
		" Папка             [Папка для файлов]  D:/Music/sf2_player",
		" Формат            [FLAC] [OGG] [MIDI]  запись аудиопотока",
		" Запись            [Старт] [Стоп] [Отмена]  записей ещё не было",
		"",
		"",
		"",
		"",
		"",
		" голоса   7/64   cpu   3%  xrun     0                          движок: SF2 · GeneralUser GS.sf2",
	];
	let lines = lines_of(&rendered_ru(&view()));
	// The title carries the package version; everything else is literal.
	let title = expected[0].replace("0.1.0", env!("CARGO_PKG_VERSION"));
	assert_eq!(lines[0].chars().count(), usize::from(MIN_COLS));
	assert_eq!(lines[0], title);
	assert_eq!(lines[1..], expected[1..], "\n{}", lines.join("\n"));
}

#[test]
fn english_screen_uses_the_dictionary_for_every_label() {
	let buf = rendered(&App::new("en"), &view());
	for text in [
		"MIDI SOURCES ──",
		" Keyboard ",
		" MIDI input        [MIDI input]  Arturia KeyStep 37  open",
		"   → notes ",
		" MIDI file         [MIDI file]  no file selected  [Start] [Pause] [Stop]",
		"PROCESSOR  [Synth] [Sound Font] ──",
		" Sound Font file   [Sound Font file]  GeneralUser GS.sf2  OK",
		" Preset            [Preset]  000:000 Grand Piano",
		" Effects           [Effects]  rev 40  cho off",
		"OUTPUT ──",
		" Audio output      [Audio output]  Speakers  48000 Hz / 256 frames  OK",
		" Master            [−] ",
		"AUDIO FILE ──",
		" Folder            [Folder for files]  D:/Music/sf2_player",
		" Format            [FLAC] [OGG] [MIDI]  records the audio stream",
		" Recording         [Start] [Stop] [Cancel]  no takes yet",
		" voices   7/64   cpu   3%  xrun     0",
		"engine: SF2 · GeneralUser GS.sf2",
		"oct 4 vel 100",
	] {
		assert!(is_shown(&buf, text), "{text:?} is missing:\n{}", lines_of(&buf).join("\n"));
	}
}

#[test]
fn healthy_settings_are_marked_green_and_labels_are_grey() {
	let buf = rendered_ru(&view());
	assert_eq!(colour_of(&buf, "открыто"), GOOD);
	let (row, col) = find(&buf, "GeneralUser GS.sf2  OK");
	assert_eq!(buf.cell(col, row).fg, TEXT, "the bank name is plain text");
	assert_eq!(
		buf.cell(col + "GeneralUser GS.sf2  ".chars().count(), row).fg,
		GOOD,
		"OK after the bank"
	);
	let (row, col) = find(&buf, "кадров  OK");
	assert_eq!(
		buf.cell(col + "кадров  ".chars().count(), row).fg,
		GOOD,
		"OK after the audio device"
	);
	assert_eq!(colour_of(&buf, "48000 Hz"), HINT);
	assert_eq!(colour_of(&buf, "Клавиатура"), LABEL);
	assert_eq!(colour_of(&buf, "ИСТОЧНИКИ MIDI"), LABEL);
	assert_eq!(colour_of(&buf, "─"), RULE);
	assert_eq!(colour_of(&buf, "движок:"), VALUE);
	assert_eq!(buf.cell(0, 1).bg, BACKGROUND);
}

/// How to break one setting, the label of its line, and the error text expected there.
type BrokenSetting = (Box<dyn Fn(&mut View)>, &'static str, &'static str);

#[test]
fn a_broken_setting_is_red_in_its_own_line_and_recovers() {
	let cases: [BrokenSetting; 7] = [
		(Box::new(|view| view.midi = MidiStatus::NoPort), "MIDI-вход  ", "нет устройства"),
		(
			Box::new(|view| view.midi = MidiStatus::Disconnected("KeyStep".into())),
			"MIDI-вход  ",
			"KeyStep (отключено)",
		),
		(
			Box::new(|view| view.notice = Some(Notice::MidiError("port busy".into()))),
			"MIDI-вход  ",
			"port busy",
		),
		(Box::new(|view| view.audio = None), "Аудио выход  ", "нет устройства"),
		(
			Box::new(|view| view.notice = Some(Notice::AudioError("stream failed".into()))),
			"Аудио выход  ",
			"stream failed",
		),
		(
			Box::new(|view| view.notice = Some(Notice::Sf2Error("not a RIFF file".into()))),
			"Файл Sound Font  ",
			"not a RIFF file",
		),
		(
			Box::new(|view| view.notice = Some(Notice::RecordError("disk full".into()))),
			"Запись  ",
			"disk full",
		),
	];
	for (break_it, line_label, error) in cases {
		let mut broken = view();
		break_it(&mut broken);
		let buf = rendered_ru(&broken);
		let (error_row, _) = find(&buf, error);
		let (label_row, _) = find(&buf, line_label);
		assert_eq!(error_row, label_row, "{error:?} is shown in its own line");
		assert_eq!(colour_of(&buf, error), ERROR, "{error:?} is red");
		assert!(!is_shown(&rendered_ru(&view()), error), "{error:?} disappears once fixed");
	}
}

#[test]
fn an_error_replaces_the_green_mark_of_its_line() {
	let mut broken = view();
	broken.notice = Some(Notice::AudioError("stream failed".into()));
	assert!(!is_shown(&rendered_ru(&broken), "кадров  OK"));
	broken.notice = Some(Notice::Sf2Error("not a RIFF file".into()));
	assert!(!is_shown(&rendered_ru(&broken), "GeneralUser GS.sf2  OK"));
	assert!(is_shown(&rendered_ru(&broken), "кадров  OK"), "the audio line is not affected");
}

#[test]
fn without_a_bank_the_file_line_is_red_and_the_preset_is_unavailable() {
	let mut no_bank = view();
	no_bank.bank = None;
	let buf = rendered_ru(&no_bank);
	assert_eq!(colour_of(&buf, "не указан"), ERROR);
	assert!(is_shown(&buf, " Пресет            [Пресет]  —"));
	assert_eq!(colour_of(&buf, "[Пресет]"), DISABLED);
	assert!(is_shown(&buf, "движок: SF2 · —"));
	assert_eq!(colour_of(&rendered_ru(&view()), "[Пресет]"), TEXT);
}

#[test]
fn buttons_show_what_is_chosen_and_what_is_unavailable() {
	let buf = rendered_ru(&view());
	let (row, col) = find(&buf, "[FLAC]");
	let chosen = buf.cell(col, row);
	assert_eq!(
		(chosen.fg, chosen.bg),
		(BACKGROUND, VALUE),
		"the chosen format is inverted on amber"
	);
	assert!(chosen.flags.contains(CellFlags::BOLD));
	let (row, col) = find(&buf, "[OGG]");
	assert_eq!((buf.cell(col, row).fg, buf.cell(col, row).bg), (TEXT, BACKGROUND));
	assert_eq!(colour_of(&buf, "[MIDI]"), TEXT, "not chosen, available");
	assert_eq!(colour_of(&buf, "[Synth]"), TEXT, "the other processor can be chosen");
	let (row, col) = find(&buf, "[Sound Font]");
	assert_eq!(buf.cell(col, row).bg, VALUE);
	assert_eq!(colour_of(&buf, "[MIDI-файл]"), TEXT, "a file can be chosen");
	assert_eq!(colour_of(&buf, "[Пауза]"), DISABLED);
	assert_eq!(colour_of(&buf, "[MIDI-вход]"), TEXT);

	let mut vorbis = view();
	vorbis.record_format = RecordFormat::OggVorbis;
	let buf = rendered_ru(&vorbis);
	let (row, col) = find(&buf, "[OGG]");
	assert_eq!(buf.cell(col, row).bg, VALUE);
	let (row, col) = find(&buf, "[FLAC]");
	assert_eq!(buf.cell(col, row).bg, BACKGROUND);
}

#[test]
fn recording_shows_a_blinking_red_indicator_and_swaps_the_available_buttons() {
	let idle = rendered_ru(&view());
	let (row, _) = find(&idle, " Запись ");
	let start_col = usize::from(VALUE_X);
	assert_eq!(idle.cell(start_col, row).fg, TEXT, "Start is available when idle");
	assert_eq!(colour_of(&idle, "[Стоп] [Отмена]"), DISABLED, "Stop is unavailable when idle");
	assert_eq!(colour_of(&idle, "записей ещё не было"), HINT);

	let mut app = App::new("ru");
	app.recording = Some(Duration::from_millis(42_100));
	let lit = rendered(&app, &view());
	assert!(is_shown(&lit, "[Отмена]  ● ЗАП 00:42"));
	assert_eq!(colour_of(&lit, "● ЗАП"), ERROR);
	assert_eq!(lit.cell(start_col, row).fg, DISABLED, "Start is unavailable while recording");
	assert_eq!(colour_of(&lit, "[Стоп] [Отмена]"), TEXT, "Stop is available while recording");
	assert!(!is_shown(&lit, "записей ещё не было"));

	// The dot is lit on even half-seconds; the text does not move when it goes dark.
	app.recording = Some(Duration::from_millis(42_600));
	let dark = rendered(&app, &view());
	assert!(is_shown(&dark, "[Отмена]    ЗАП 00:42"));
	app.recording = Some(Duration::from_secs(3_723));
	assert!(is_shown(&rendered(&app, &view()), "ЗАП 62:03"));
	assert_eq!(record_dot(Duration::from_millis(499)), RECORD_DOT);
	assert_eq!(record_dot(Duration::from_millis(500)), " ");
	assert_eq!(record_dot(Duration::from_millis(1_000)), RECORD_DOT);
}

#[test]
fn level_meters_fill_by_decibels_and_colour_cells_by_zone() {
	let cells = usize::from(METER_CELLS);
	// Zone boundaries on 26 cells: amber from cell 21 (21/26 ≥ 0.78), red from cell 24 (24/26 ≥ 0.92).
	let mut loud = view();
	loud.levels = Levels {
		peak_left: 1.0,
		rms_left: 1.0,
		peak_right: 0.001,
		rms_right: 0.001,
	};
	let buf = rendered_ru(&loud);
	assert!(is_shown(&buf, &format!(" L                 {}     0.0 dBFS", "█".repeat(cells))));
	let left = meter_colours(&buf, " L ", cells);
	assert!(left[..21].iter().all(|colour| *colour == GOOD), "cells 0..=20 are green");
	assert!(left[21..24].iter().all(|colour| *colour == VALUE), "cells 21..=23 are amber");
	assert!(left[24..].iter().all(|colour| *colour == ERROR), "cells 24..=25 are red");

	// -60 dBFS is the floor of the bar: nothing is lit, the number still shows the peak.
	assert!(is_shown(&buf, &format!(" R                 {}   -60.0 dBFS", "░".repeat(cells))));
	assert!(meter_colours(&buf, " R ", cells).iter().all(|colour| *colour == METER_TRACK));

	// -30 dBFS is half of the 60 dB scale: 13 of 26 cells.
	let mut half = view();
	half.levels.rms_left = 0.031_622_78;
	half.levels.peak_left = 0.5;
	let buf = rendered_ru(&half);
	assert!(is_shown(
		&buf,
		&format!(" L                 {}{}    -6.0 dBFS", "█".repeat(13), "░".repeat(13))
	));
}

#[test]
fn clipping_and_mute_are_marked_next_to_their_meters() {
	let mut app = App::new("ru");
	assert!(!is_shown(&rendered(&app, &view()), "CLIP"));
	app.clipping = true;
	let buf = rendered(&app, &view());
	assert!(is_shown(&buf, "dBFS  CLIP"));
	assert_eq!(colour_of(&buf, "CLIP"), ERROR);

	let mut muted = view();
	muted.mixer.toggle_master_mute();
	let buf = rendered_ru(&muted);
	assert!(is_shown(&buf, " 80% [+]  тихо"));
	assert_eq!(colour_of(&buf, "тихо"), MESSAGE);
	assert!(!is_shown(&rendered_ru(&view()), "тихо"));

	let mut full = view();
	full.mixer = Mixer::new(100);
	assert!(is_shown(&rendered_ru(&full), &format!("[−] {} 100% [+]", "█".repeat(20))));
}

#[test]
fn status_counters_keep_their_width_and_optional_fields_follow_them() {
	let counters = |held: u32, xruns: u32, load: u32| {
		let mut busy = view();
		busy.held_notes = held;
		busy.xruns = xruns;
		busy.load_percent = load;
		let buf = rendered_ru(&busy);
		let (row, _) = find(&buf, "голоса");
		let line = lines_of(&buf)[row].clone();
		line.chars().take(38).collect::<String>()
	};
	assert_eq!(counters(0, 0, 0), " голоса   0/64   cpu   0%  xrun     0 ");
	assert_eq!(counters(999, 99_999, 100), " голоса 999/64   cpu 100%  xrun 99999 ");
	assert_eq!(counters(5_000, 100_000, 250), " голоса 999/64   cpu 100%  xrun 9999+ ");

	let mut no_engine = view();
	no_engine.max_voices = 0;
	assert!(is_shown(&rendered_ru(&no_engine), " голоса   -/-    cpu"));

	let mut busy = view();
	busy.transpose = 3;
	busy.omni_channel = Some(0);
	busy.dropped = 2;
	busy.record_dropped = 7;
	let buf = rendered_ru(&busy);
	assert!(is_shown(&buf, "xrun     0  тр +3  omni к1  потеряно 2  зап. потеряно 7"));
	// The counters win: the processor summary gives way when the row is full.
	busy.bank = Some(BankView {
		file_name: "A very long SoundFont bank file name indeed.sf2".into(),
		preset_label: String::new(),
	});
	let buf = rendered_ru(&busy);
	assert!(is_shown(&buf, "зап. потеряно 7"));
	assert!(!is_shown(&buf, "движок:"));
}

#[test]
fn notices_not_tied_to_a_setting_go_to_the_message_line_in_yellow() {
	let mut saved = view();
	saved.notice = Some(Notice::RecordSaved("2026-11-13_14-02-11.flac".into()));
	let buf = rendered_ru(&saved);
	let (row, _) = find(&buf, "2026-11-13_14-02-11.flac");
	let (record_row, _) = find(&buf, " Запись ");
	assert_eq!(row, record_row + 1, "the message line sits under the recording row");
	assert_eq!(buf.cell(usize::from(LABEL_X), row).fg, MESSAGE);

	let mut font = view();
	font.notice = Some(Notice::FontFallback("Comic Mono".into()));
	assert!(is_shown(&rendered_ru(&font), "Шрифт Comic Mono недоступен"));
	assert!(is_shown(&rendered(&App::new("en"), &font), "Font Comic Mono is unavailable"));
}

#[test]
fn a_grid_below_the_minimum_shows_only_the_hint() {
	for (cols, rows) in [(MIN_COLS - 1, MIN_ROWS), (MIN_COLS, MIN_ROWS - 1), (20, 5)] {
		let mut buf = screen(usize::from(cols), usize::from(rows));
		render(&mut buf, &App::new("ru"), &view(), &mut Buttons::new());
		let lines = lines_of(&buf);
		let hint: String = "Увеличьте окно до 96×31".chars().take(usize::from(cols)).collect();
		assert_eq!(lines[0], hint);
		assert_eq!(buf.cell(0, 0).fg, ERROR);
		assert!(lines[1..].iter().all(String::is_empty), "nothing else is drawn at {cols}×{rows}");
	}

	// A larger grid keeps the layout: the piano stays centred, the status goes to the last row.
	let mut wide = screen(120, 40);
	render(&mut wide, &App::new("ru"), &view(), &mut Buttons::new());
	let (piano_row, piano_col) = find(&wide, "│ ██ ██  │ ██ ██ ██  │ ██ ██  │");
	assert_eq!(piano_col, (120 - usize::from(piano::WIDTH)) / 2);
	assert_eq!(
		piano_origin(wide.area()),
		Some((piano_col as u16, piano_row as u16)),
		"the mouse looks for the piano where it is drawn"
	);
	let (status_row, _) = find(&wide, "голоса");
	assert_eq!(status_row, 39);
	assert_eq!(lines_of(&wide)[0].chars().count(), 120, "the title rule spans the whole width");
}

#[test]
fn a_frame_drawn_over_another_leaves_no_leftovers() {
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(5));
	app.clipping = true;
	let mut noisy = view();
	noisy.notice = Some(Notice::RecordSaved("take.flac".into()));
	noisy.record_format = RecordFormat::OggVorbis;
	let mut buttons = Buttons::new();
	render(&mut buf, &app, &noisy, &mut buttons);
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	let clean = rendered_ru(&view());
	for row in 0..buf.rows() {
		for col in 0..buf.cols() {
			assert_eq!(buf.cell(col, row), clean.cell(col, row), "cell {col},{row}");
		}
	}
}

/// A left click, press and release, on the first cell of `text`; what the release did.
fn click(buttons: &mut Buttons, buf: &ScreenBuffer, text: &str) -> Touch {
	let (row, col) = find(buf, text);
	let at = |kind| MouseMsg {
		kind,
		button: Some(MouseButton::Left),
		col: i32::try_from(col).expect("column"),
		row: i32::try_from(row).expect("row"),
		modifiers: ModifiersState::empty(),
		click_count: 1,
	};
	buttons.handle_mouse(&at(MouseKind::Down));
	buttons.handle_mouse(&at(MouseKind::Up))
}

/// The buttons Tab stops at, starting from the one under the focus, once round the ring.
fn ring(buttons: &mut Buttons) -> Vec<ButtonId> {
	let mut stops = Vec::new();
	while let Some(focused) = buttons.focused().filter(|focused| !stops.contains(focused)) {
		stops.push(focused);
		buttons.handle_key(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
	}
	stops
}

#[test]
fn every_button_of_the_screen_is_pressed_by_a_click_and_by_tab_and_enter() {
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	// The text of a button as it stands on the screen, with enough around it to be unique.
	let available = [
		(ButtonId::MidiIn, "[MIDI-вход]"),
		(ButtonId::MidiFile, "[MIDI-файл]"),
		(ButtonId::EngineSynth, "[Synth]"),
		(ButtonId::EngineSf2, "[Sound Font]"),
		(ButtonId::Sf2File, "[Файл Sound Font]"),
		(ButtonId::Preset, "[Пресет]"),
		(ButtonId::Effects, "[Эффекты]"),
		(ButtonId::AudioOut, "[Аудио выход]"),
		(ButtonId::MasterDown, "[−]"),
		(ButtonId::MasterUp, "[+]"),
		(ButtonId::Folder, "[Папка для файлов]"),
		(ButtonId::FormatFlac, "[FLAC]"),
		(ButtonId::FormatOgg, "[OGG]"),
		(ButtonId::FormatMidi, "[MIDI]"),
		(ButtonId::RecordStart, "[Старт] [Стоп] [Отмена]"),
	];
	for (id, text) in available {
		assert_eq!(click(&mut buttons, &buf, text), Touch::Pressed(id), "{text}");
		assert_eq!(buttons.focused(), Some(id), "a click on {text} moves the focus there");
	}

	// Tab goes through the same buttons top to bottom, left to right; Enter presses each.
	click(&mut buttons, &buf, "[MIDI-вход]");
	let order: Vec<ButtonId> = available.iter().map(|(id, _)| *id).collect();
	assert_eq!(ring(&mut buttons), order);
	for id in order {
		assert_eq!(
			buttons.handle_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
			Touch::Pressed(id)
		);
		buttons.handle_key(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
	}

	// What the screen shows unavailable does not react.
	for text in ["[Старт] [Пауза] [Стоп]", "[Пауза]", "[Стоп]", "[Стоп] [Отмена]", "[Отмена]"] {
		assert_eq!(click(&mut buttons, &buf, text), Touch::Missed, "{text}");
	}
}

#[test]
fn the_button_under_the_focus_is_underlined_from_the_first_frame() {
	let buf = rendered_ru(&view());
	let is_underlined = |text: &str| {
		let (row, col) = find(&buf, text);
		buf.cell(col, row).flags.contains(CellFlags::UNDERLINE)
	};
	assert!(is_underlined("[MIDI-вход]"));
	assert!(!is_underlined("[Файл Sound Font]") && !is_underlined("[FLAC]") && !is_underlined("[MIDI-файл]"));
}

#[test]
fn a_running_take_locks_the_format_and_takes_start_away() {
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(3));
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &app, &view(), &mut buttons);
	// The chosen format stays lit, the other goes dark, and neither reacts.
	let (row, col) = find(&buf, "[FLAC]");
	assert_eq!((buf.cell(col, row).fg, buf.cell(col, row).bg), (BACKGROUND, VALUE));
	assert_eq!(colour_of(&buf, "[OGG]"), DISABLED);
	for text in ["[FLAC]", "[OGG]", "[MIDI]", "[Старт] [Стоп] [Отмена]"] {
		assert_eq!(click(&mut buttons, &buf, text), Touch::Missed, "{text}");
	}
	assert_eq!(click(&mut buttons, &buf, "[Стоп] [Отмена]"), Touch::Pressed(ButtonId::RecordStop));
	let stops = ring(&mut buttons);
	assert!(stops.contains(&ButtonId::RecordStop));
	assert!(!stops.contains(&ButtonId::RecordStart) && !stops.contains(&ButtonId::FormatFlac) && !stops.contains(&ButtonId::FormatOgg));
}

#[test]
fn on_a_grid_below_the_minimum_no_button_reacts() {
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	assert_eq!(buttons.focused(), Some(ButtonId::MidiIn));
	let mut small = screen(usize::from(MIN_COLS) - 1, usize::from(MIN_ROWS));
	render(&mut small, &App::new("ru"), &view(), &mut buttons);
	assert_eq!(buttons.focused(), None);
	assert_eq!(
		buttons.handle_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
		Touch::Missed
	);
	// Where `[MIDI-вход]` stood on the full screen.
	assert_eq!(click(&mut buttons, &buf, "[MIDI-вход]"), Touch::Missed);
}

/// A note map with the given MIDI keys set.
fn sounding(keys: &[u8]) -> u128 {
	keys.iter().fold(0, |map, key| map | (1u128 << key))
}

#[test]
fn the_note_rows_name_what_each_source_plays_and_the_piano_lights_both() {
	let mut playing = view();
	playing.keyboard_notes = sounding(&[60, 64, 67]);
	playing.midi_notes = sounding(&[62, 61, 100]);
	let buf = rendered_ru(&playing);
	assert!(is_shown(&buf, " Клавиатура        C4 E4 G4 "), "the names of the window's notes");
	assert!(is_shown(&buf, "   → ноты          C#4 D4 E7"), "lowest first, sharps named");
	assert_eq!(colour_of(&buf, "C4 E4 G4"), VALUE);
	assert_eq!(colour_of(&buf, "C#4 D4 E7"), VALUE);
	assert!(is_shown(&buf, "окт 4 vel 100"), "the read-out of the keyboard row stays in place");
	// The piano lights the keys of both sources that are in its range: C C# D E G; E7 is outside.
	assert!(is_shown(&buf, "│██│██│██│  │██│  │  │  │  │  │"));
	let (row, col) = piano_origin(buf.area())
		.map(|(x, y)| (usize::from(y), usize::from(x)))
		.expect("shown");
	assert_eq!(buf.cell(col + 2, row).fg, VALUE, "C# is lit");
	assert_eq!(buf.cell(col + 5, row).fg, HINT, "D# is not");

	// Silence: a dash in both rows, a grey piano.
	let buf = rendered_ru(&view());
	assert!(is_shown(&buf, " Клавиатура        —"));
	assert!(is_shown(&buf, "   → ноты          —"));
	assert!(is_shown(&buf, "│  │  │  │  │  │  │  │  │  │  │"));
}

#[test]
fn the_piano_follows_the_base_octave_of_the_text_keyboard() {
	let mut playing = view();
	playing.midi_notes = sounding(&[60]);
	let mut app = App::new("ru");
	assert!(
		is_shown(&rendered(&app, &playing), "│██│  │  │  │  │  │  │  │  │  │"),
		"C4 is the first key"
	);
	app.keyboard.octave_down();
	assert!(
		is_shown(&rendered(&app, &playing), "│  │  │  │  │  │  │  │██│  │  │"),
		"from C3 it is the eighth"
	);
	app.keyboard.octave_up();
	app.keyboard.octave_up();
	assert!(
		is_shown(&rendered(&app, &playing), "│  │  │  │  │  │  │  │  │  │  │"),
		"from C5 it is out of range"
	);
}

#[test]
fn a_long_list_of_notes_is_cut_at_a_whole_name_and_marked() {
	let mut playing = view();
	// Twenty-four notes from C2 up.
	let many: Vec<u8> = (36..60).collect();
	playing.keyboard_notes = sounding(&many);
	playing.midi_notes = sounding(&many);
	let buf = rendered_ru(&playing);
	let lines = lines_of(&buf);
	let (keyboard_row, _) = find(&buf, " Клавиатура ");
	let keyboard: String = lines[keyboard_row].chars().skip(usize::from(VALUE_X)).take(50).collect();
	// 48 cells are the keyboard row's: up to two short of the read-out at column 69. C#3 and the mark
	// would take 49.
	assert_eq!(keyboard, "C2 C#2 D2 D#2 E2 F2 F#2 G2 G#2 A2 A#2 B2 C3 …     ");
	assert!(
		lines[keyboard_row].contains("C3 …     окт 4 vel 100"),
		"the read-out is not overwritten"
	);
	// The MIDI row has the whole width to the right edge.
	let (midi_row, _) = find(&buf, "   → ноты ");
	let midi: String = lines[midi_row].chars().skip(usize::from(VALUE_X)).collect();
	assert_eq!(midi, "C2 C#2 D2 D#2 E2 F2 F#2 G2 G#2 A2 A#2 B2 C3 C#3 D3 D#3 E3 F3 F#3 G3 G#3 A3 …");
	assert!(
		lines[midi_row].chars().count() < usize::from(MIN_COLS),
		"one cell is left at the edge"
	);

	// Twenty-six natural notes from C2 take exactly 77 cells — one more than the row has: the last cell
	// of the screen stays free, so the list is cut.
	let naturals: Vec<u8> = (36..128)
		.filter(|key| [0, 2, 4, 5, 7, 9, 11].contains(&(key % 12)))
		.take(26)
		.collect();
	playing.midi_notes = sounding(&naturals);
	let buf = rendered_ru(&playing);
	let midi: String = lines_of(&buf)[midi_row].chars().skip(usize::from(VALUE_X)).collect();
	assert!(midi.ends_with("F5 …") && !midi.contains("G5"), "{midi}");
	assert!(midi.chars().count() <= 76);
	// One note fewer fits whole.
	playing.midi_notes = sounding(&naturals[..25]);
	let buf = rendered_ru(&playing);
	assert!(lines_of(&buf)[midi_row].ends_with("E5 F5"));
}

/// The view with the built-in synthesizer chosen.
fn synth_view() -> View {
	let mut view = view();
	view.engine = EngineKind::Builtin;
	view
}

#[test]
fn the_synth_screen_matches_the_layout_mockup() {
	let expected = [
		"ОБРАБОТЧИК  [Synth] [Sound Font] ───────────────────────────────────────────────────────────────",
		" Форма сигнала     [Синусоида] [Пила] [Треугольник]",
		" ADSR              пресет: [Пианино] [Клавесин] [Орган] [Флейта]",
		"   значения        Attack: [−] 0.003 с [+]   Decay: [−] 0.500 с [+]",
		"                   Sustain: [−]  35% [+]     Release: [−] 0.350 с [+]",
		" Эффекты           [Эффекты]  рев 40  хор выкл",
		"",
		"ВЫХОД ──────────────────────────────────────────────────────────────────────────────────────────",
		" Аудио выход       [Аудио выход]  Speakers  48000 Hz / 256 кадров  OK",
		" L                 ░░░░░░░░░░░░░░░░░░░░░░░░░░      −∞ dBFS",
		" R                 ░░░░░░░░░░░░░░░░░░░░░░░░░░      −∞ dBFS",
		" Мастер            [−] ████████████████░░░░  80% [+]",
		"",
		"АУДИО-ФАЙЛ ─────────────────────────────────────────────────────────────────────────────────────",
		" Папка             [Папка для файлов]  D:/Music/sf2_player",
		" Формат            [FLAC] [OGG] [MIDI]  запись аудиопотока",
		" Запись            [Старт] [Стоп] [Отмена]  записей ещё не было",
		"",
		"",
		"",
		" голоса   7/64   cpu   3%  xrun     0                                 движок: Synth · Синусоида",
	];
	let buf = rendered_ru(&synth_view());
	let lines = lines_of(&buf);
	// The sections above the processor are the same for both; the synthesizer takes two rows more,
	// and the screen still fits the smallest grid.
	assert_eq!(lines[..10], lines_of(&rendered_ru(&view()))[..10]);
	assert_eq!(lines[10..], expected, "{}", lines[10..].join("|"));
	assert_eq!(lines.len(), usize::from(MIN_ROWS));
}

#[test]
fn the_processor_buttons_show_which_one_plays_and_each_has_its_own_rows() {
	let sound_font = rendered_ru(&view());
	let synth = rendered_ru(&synth_view());
	let background = |buf: &ScreenBuffer, text: &str| {
		let (row, col) = find(buf, text);
		buf.cell(col, row).bg
	};
	assert_eq!(
		(background(&sound_font, "[Synth]"), background(&sound_font, "[Sound Font]")),
		(BACKGROUND, VALUE)
	);
	assert_eq!(
		(background(&synth, "[Synth]"), background(&synth, "[Sound Font]")),
		(VALUE, BACKGROUND)
	);
	for text in ["Файл Sound Font", "[Пресет]"] {
		assert!(
			is_shown(&sound_font, text) && !is_shown(&synth, text),
			"{text} belongs to the Sound Font"
		);
	}
	for text in ["Форма сигнала", "ADSR", "Attack:"] {
		assert!(
			is_shown(&synth, text) && !is_shown(&sound_font, text),
			"{text} belongs to the synthesizer"
		);
	}
	// The effects row is shared, the summary names the processor.
	assert!(is_shown(&synth, "[Эффекты]  рев 40  хор выкл"));
	assert!(is_shown(&sound_font, "движок: SF2 · GeneralUser GS.sf2"));
	// The synthesizer needs no bank: without one nothing on its screen is red.
	let mut bare = synth_view();
	bare.bank = None;
	let buf = rendered_ru(&bare);
	let has_red = (0..buf.rows()).any(|row| (0..buf.cols()).any(|col| buf.cell(col, row).fg == ERROR));
	assert!(!has_red);
}

#[test]
fn the_synth_rows_show_the_timbre_and_mark_a_hand_made_envelope() {
	let mut timbre = synth_view();
	timbre.builtin.wave = Wave::Saw;
	timbre.builtin.apply_preset(AdsrPreset::Organ);
	let buf = rendered_ru(&timbre);
	let is_lit = |buf: &ScreenBuffer, text: &str| {
		let (row, col) = find(buf, text);
		buf.cell(col, row).bg == VALUE
	};
	assert!(is_lit(&buf, "[Пила]") && !is_lit(&buf, "[Синусоида]") && !is_lit(&buf, "[Треугольник]"));
	assert!(is_lit(&buf, "[Орган]") && !is_lit(&buf, "[Пианино]"));
	assert!(is_shown(&buf, "Attack: [−] 0.030 с [+]   Decay: [−] 0.060 с [+]"));
	assert!(is_shown(&buf, "Sustain: [−] 100% [+]     Release: [−] 0.090 с [+]"));
	assert_eq!(colour_of(&buf, "0.030 с"), VALUE);
	assert!(!is_shown(&buf, "Своё"));
	assert!(is_shown(&buf, "движок: Synth · Пила"));

	// A value moved by hand: no preset is lit, the envelope is marked as the user's own.
	timbre.builtin.adjust(AdsrField::Sustain, -1);
	let buf = rendered_ru(&timbre);
	assert!(!is_lit(&buf, "[Орган]"));
	assert!(is_shown(&buf, "[Флейта]  Своё"));
	assert_eq!(colour_of(&buf, "Своё"), VALUE);
	assert!(
		is_shown(&buf, "Sustain: [−]  95% [+]     Release:"),
		"the second value stays in its column"
	);

	// The longest values still leave the columns where they are.
	timbre.builtin.sustain = 0.05;
	timbre.builtin.attack = 2.0;
	let buf = rendered_ru(&timbre);
	assert!(is_shown(&buf, "Attack: [−] 2.000 с [+]   Decay:"));
	assert!(is_shown(&buf, "Sustain: [−]   5% [+]     Release:"));

	let english = rendered(&App::new("en"), &synth_view());
	for text in [
		" Waveform          [Sine] [Saw] [Triangle]",
		" ADSR              preset: [Piano] [Harpsichord] [Organ] [Flute]",
		"   values          Attack: [−] 0.003 s [+]   Decay: [−] 0.500 s [+]",
		"engine: Synth · Sine",
	] {
		assert!(is_shown(&english, text), "{text:?}");
	}
}

#[test]
fn every_button_of_the_synth_section_is_pressed_by_a_click_and_stands_in_the_tab_ring() {
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &synth_view(), &mut buttons);
	let section = [
		(ButtonId::EngineSynth, "[Synth]"),
		(ButtonId::EngineSf2, "[Sound Font]"),
		(ButtonId::WaveSine, "[Синусоида]"),
		(ButtonId::WaveSaw, "[Пила]"),
		(ButtonId::WaveTriangle, "[Треугольник]"),
		(ButtonId::AdsrPiano, "[Пианино]"),
		(ButtonId::AdsrHarpsichord, "[Клавесин]"),
		(ButtonId::AdsrOrgan, "[Орган]"),
		(ButtonId::AdsrFlute, "[Флейта]"),
		(ButtonId::AttackDown, "[−] 0.003"),
		(ButtonId::AttackUp, "[+]   Decay"),
		(ButtonId::DecayDown, "[−] 0.500"),
		(ButtonId::DecayUp, "[+]"),
		(ButtonId::SustainDown, "[−]  35%"),
		(ButtonId::SustainUp, "[+]     Release"),
		(ButtonId::ReleaseDown, "[−] 0.350"),
		(ButtonId::Effects, "[Эффекты]"),
	];
	for (id, text) in section {
		if id == ButtonId::DecayUp {
			continue;
		}
		assert_eq!(click(&mut buttons, &buf, text), Touch::Pressed(id), "{text}");
	}
	// The two buttons that end a row are found by their place: right after their value.
	let after = |text: &str| {
		let (row, col) = find(&buf, text);
		(row, col + text.chars().count() + 1)
	};
	for (id, value) in [(ButtonId::DecayUp, "0.500 с"), (ButtonId::ReleaseUp, "0.350 с")] {
		let (row, col) = after(value);
		let at = |kind| MouseMsg {
			kind,
			button: Some(MouseButton::Left),
			col: i32::try_from(col).expect("column"),
			row: i32::try_from(row).expect("row"),
			modifiers: ModifiersState::empty(),
			click_count: 1,
		};
		buttons.handle_mouse(&at(MouseKind::Down));
		assert_eq!(buttons.handle_mouse(&at(MouseKind::Up)), Touch::Pressed(id));
	}

	// Tab walks them between the MIDI input and the output, in the order they stand on the screen;
	// the buttons of the Sound Font are not in the ring.
	click(&mut buttons, &buf, "[MIDI-вход]");
	let stops = ring(&mut buttons);
	assert_eq!(
		stops[..20],
		[
			ButtonId::MidiIn,
			ButtonId::MidiFile,
			ButtonId::EngineSynth,
			ButtonId::EngineSf2,
			ButtonId::WaveSine,
			ButtonId::WaveSaw,
			ButtonId::WaveTriangle,
			ButtonId::AdsrPiano,
			ButtonId::AdsrHarpsichord,
			ButtonId::AdsrOrgan,
			ButtonId::AdsrFlute,
			ButtonId::AttackDown,
			ButtonId::AttackUp,
			ButtonId::DecayDown,
			ButtonId::DecayUp,
			ButtonId::SustainDown,
			ButtonId::SustainUp,
			ButtonId::ReleaseDown,
			ButtonId::ReleaseUp,
			ButtonId::Effects,
		]
	);
	assert!(!stops.contains(&ButtonId::Sf2File) && !stops.contains(&ButtonId::Preset));
}

/// The view with a MIDI file of 84 seconds chosen, 28 seconds into it.
fn file_view(transport: Transport) -> View {
	let mut view = view();
	view.file = FileView {
		file: MidiFile::Loaded("D:/midi/bach_bwv_846_prelude.mid".into()),
		transport,
		position_tenths: if transport == Transport::Stopped { 0 } else { 280 },
		duration_tenths: 840,
	};
	view
}

/// The foreground colours of the three transport buttons of the MIDI file row.
fn transport_colours(buf: &ScreenBuffer) -> [Color; 3] {
	let (row, col) = find(buf, "[Старт] [Пауза] [Стоп]");
	[0, 8, 16].map(|offset| buf.cell(col + offset, row).fg)
}

#[test]
fn a_chosen_midi_file_shows_its_name_and_the_buttons_that_apply() {
	let stopped = rendered_ru(&file_view(Transport::Stopped));
	assert!(is_shown(
		&stopped,
		" MIDI-файл         [MIDI-файл]  bach_bwv_846_prelude.mid  [Старт] [Пауза] [Стоп]"
	));
	assert_eq!(colour_of(&stopped, "bach_bwv_846_prelude.mid"), TEXT, "the name without its folder");
	assert_eq!(
		transport_colours(&stopped),
		[TEXT, DISABLED, DISABLED],
		"a stopped file can only be started"
	);
	assert!(is_shown(
		&stopped,
		"   позиция         [························]  00:00 / 01:24  остановлен"
	));
	assert_eq!(colour_of(&stopped, "остановлен"), HINT);

	let playing = rendered_ru(&file_view(Transport::Playing));
	assert_eq!(
		transport_colours(&playing),
		[DISABLED, TEXT, TEXT],
		"a playing file can be paused or stopped"
	);
	// 28 of 84 seconds: a third of the 24 cells.
	assert!(is_shown(
		&playing,
		"   позиция         [████████················]  00:28 / 01:24  воспроизведение"
	));
	assert_eq!(colour_of(&playing, "воспроизведение"), GOOD);
	let (row, col) = find(&playing, "[████████");
	assert_eq!(playing.cell(col + 1, row).fg, VALUE, "the played part is amber");
	assert_eq!(playing.cell(col + 9, row).fg, METER_TRACK);

	let paused = rendered_ru(&file_view(Transport::Paused));
	assert_eq!(
		transport_colours(&paused),
		[TEXT, DISABLED, TEXT],
		"a paused file can go on or be stopped"
	);
	assert!(is_shown(&paused, "00:28 / 01:24  пауза"));
	assert_eq!(colour_of(&paused, "пауза"), MESSAGE);

	let english = rendered(&App::new("en"), &file_view(Transport::Playing));
	assert!(is_shown(
		&english,
		"   position        [████████················]  00:28 / 01:24  playing"
	));
}

#[test]
fn the_position_row_is_there_only_for_a_chosen_file_and_counts_long_files_in_minutes() {
	assert!(!is_shown(&rendered_ru(&view()), "позиция"), "no file, no position");
	let mut long = file_view(Transport::Playing);
	// 75 minutes and 9 seconds into a file of two hours: minutes run past 59.
	long.file.position_tenths = (75 * 60 + 9) * 10 + 9;
	long.file.duration_tenths = 120 * 60 * 10;
	let buf = rendered_ru(&long);
	assert!(is_shown(&buf, "75:09 / 120:00"));
	// A file of no length does not divide by zero: the bar is empty.
	long.file.duration_tenths = 0;
	long.file.position_tenths = 0;
	assert!(is_shown(&rendered_ru(&long), "[························]  00:00 / 00:00"));
	// At the very end the bar is full.
	long.file.duration_tenths = 100;
	long.file.position_tenths = 100;
	assert!(is_shown(&rendered_ru(&long), "[████████████████████████]  00:10 / 00:10"));
}

#[test]
fn a_file_that_cannot_be_played_says_why_in_red_and_offers_no_transport() {
	let mut missing = view();
	missing.file.file = MidiFile::Missing("D:/midi/gone.mid".into());
	let buf = rendered_ru(&missing);
	assert!(is_shown(&buf, "[MIDI-файл]  файл не найден: D:/midi/gone.mid  [Старт]"));
	assert_eq!(colour_of(&buf, "файл не найден"), ERROR);
	assert_eq!(transport_colours(&buf), [DISABLED; 3]);
	assert!(!is_shown(&buf, "позиция"));
	assert!(is_shown(&rendered(&App::new("en"), &missing), "file not found: D:/midi/gone.mid"));

	let mut broken = view();
	broken.file.file = MidiFile::Broken {
		path: "D:/midi/odd.mid".into(),
		reason: "MIDI file format 2 is not supported (only 0 and 1 are)".into(),
	};
	let buf = rendered_ru(&broken);
	assert!(is_shown(
		&buf,
		"[MIDI-файл]  MIDI file format 2 is not supported (o…  [Старт] [Пауза] [Стоп]"
	));
	assert_eq!(colour_of(&buf, "MIDI file format 2"), ERROR);
	assert_eq!(
		colour_of(&rendered_ru(&view()), "файл не выбран"),
		HINT,
		"no file chosen is not an error"
	);
}

#[test]
fn a_long_file_name_is_cut_so_that_the_transport_buttons_stay_on_the_screen() {
	let mut long = file_view(Transport::Playing);
	long.file.file =
		MidiFile::Loaded("D:/midi/Johann Sebastian Bach - Das Wohltemperierte Klavier - Praeludium und Fuge C-Dur BWV 846.mid".into());
	let buf = rendered_ru(&long);
	let (row, _) = find(&buf, " MIDI-файл ");
	let line = lines_of(&buf)[row].clone();
	assert_eq!(
		line,
		" MIDI-файл         [MIDI-файл]  Johann Sebastian Bach - Das Wohltemper…  [Старт] [Пауза] [Стоп]"
	);
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 1, "up to one cell short of the edge");
	// The buttons are still where a click finds them.
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &long, &mut buttons);
	assert_eq!(click(&mut buttons, &buf, "[Пауза]"), Touch::Pressed(ButtonId::FilePause));
	assert_eq!(click(&mut buttons, &buf, "[Стоп]"), Touch::Pressed(ButtonId::FileStop));
	assert_eq!(
		click(&mut buttons, &buf, "[Старт] [Пауза]"),
		Touch::Missed,
		"it plays: Start does not apply"
	);
}

#[test]
fn the_transport_buttons_stand_in_the_tab_ring_after_the_file_button() {
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &file_view(Transport::Paused), &mut buttons);
	click(&mut buttons, &buf, "[MIDI-вход]");
	let stops = ring(&mut buttons);
	assert_eq!(
		stops[..5],
		[
			ButtonId::MidiIn,
			ButtonId::MidiFile,
			ButtonId::FileStart,
			ButtonId::FileStop,
			ButtonId::EngineSynth
		]
	);
	assert_eq!(click(&mut buttons, &buf, "[Старт] [Пауза]"), Touch::Pressed(ButtonId::FileStart));
}

#[test]
fn a_running_take_shows_its_time_size_and_file_and_offers_a_red_cancel() {
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_millis(42_100));
	let mut recording = view();
	recording.take = TakeView::Recording {
		name: "2026-11-13_12-03-41.flac".into(),
		size: Size::of(1_300_000),
	};
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &app, &recording, &mut buttons);
	assert!(is_shown(&buf, "[Отмена]  ● ЗАП 00:42 · 1.2 МБ · 2026-11-13_12-03-41.flac"));
	assert_eq!(colour_of(&buf, "● ЗАП"), ERROR);
	assert_eq!(colour_of(&buf, "2026-11-13_12-03-41.flac"), ERROR, "the whole indicator is red");
	assert_eq!(colour_of(&buf, "[Стоп] [Отмена]"), TEXT);
	assert_eq!(colour_of(&buf, "[Отмена]"), ERROR, "what cannot be undone is red");
	assert_eq!(click(&mut buttons, &buf, "[Отмена]"), Touch::Pressed(ButtonId::RecordCancel));
	assert!(ring(&mut buttons).contains(&ButtonId::RecordCancel), "Tab reaches it");

	// A name too long for the row is cut and marked, one cell short of the right edge.
	recording.take = TakeView::Recording {
		name: "x".repeat(90),
		size: Size::of(0),
	};
	let buf = rendered(&app, &recording);
	let (row, _) = find(&buf, "● ЗАП 00:42 · 0 КБ · xxx");
	let line = &lines_of(&buf)[row];
	assert!(line.ends_with("xx…"));
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 1);

	// Without a take there is nothing to cancel.
	let idle = rendered_ru(&view());
	assert_eq!(colour_of(&idle, "[Отмена]"), DISABLED);
}

#[test]
fn the_last_saved_take_is_shown_in_green_with_its_path_and_size() {
	let mut saved = view();
	saved.take = TakeView::Saved {
		path: "D:/rec/2026-11-13_12-03-41.flac".into(),
		size: Size::of(5 * 1024 * 1024 + 300 * 1024),
	};
	let buf = rendered_ru(&saved);
	println!(
		"{}",
		lines_of(&buf).join(
			"
"
		)
	);
	assert!(is_shown(&buf, "[Отмена]  сохранено: D:/rec/2026-11-13_12-03-41.flac · 5.3 МБ"));
	assert_eq!(colour_of(&buf, "сохранено:"), GOOD);
	assert!(!is_shown(&buf, "записей ещё не было"));
	assert!(is_shown(
		&rendered(&App::new("en"), &saved),
		"[Cancel]  saved: D:/rec/2026-11-13_12-03-41.flac · 5.3 MB"
	));

	// A long path loses its beginning, not the file name; the row stops one cell short of the edge.
	saved.take = TakeView::Saved {
		path: format!("D:/{}2026-11-13_12-03-41.flac", "папка/".repeat(20)),
		size: Size::TenthsOfMegabyte(12),
	};
	let buf = rendered_ru(&saved);
	let (row, _) = find(&buf, "сохранено: …");
	let line = &lines_of(&buf)[row];
	assert!(line.ends_with("папка/2026-11-13_12-03-41.flac · 1.2 МБ"), "{line}");
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 1);

	// An error of the record line is more urgent than the last take; a new take replaces both.
	saved.notice = Some(Notice::RecordError("disk full".into()));
	let buf = rendered_ru(&saved);
	assert!(!is_shown(&buf, "сохранено:"));
	assert_eq!(colour_of(&buf, "disk full"), ERROR);
}

#[test]
fn a_cancelled_take_is_reported_on_the_message_line() {
	let mut cancelled = view();
	cancelled.notice = Some(Notice::RecordCancelled);
	let buf = rendered_ru(&cancelled);
	assert_eq!(colour_of(&buf, "Запись отменена"), MESSAGE);
	assert!(is_shown(&buf, "записей ещё не было"), "a cancelled take is not a saved one");
	assert!(is_shown(&rendered(&App::new("en"), &cancelled), "Recording cancelled"));
}

#[test]
fn a_text_cut_at_the_front_keeps_its_end() {
	assert_eq!(fitted_end("abcdef", 6), "abcdef");
	assert_eq!(fitted_end("abcdef", 5), "…cdef");
	assert_eq!(fitted_end("abcdef", 1), "…");
	assert_eq!(fitted_end("abcdef", 0), "…");
	assert_eq!(fitted_end("папка", 3), "…ка");
}

#[test]
fn the_midi_format_is_chosen_like_the_others_and_says_what_it_records() {
	let mut midi = view();
	midi.record_format = RecordFormat::Midi;
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &midi, &mut buttons);
	assert!(is_shown(&buf, "[FLAC] [OGG] [MIDI]  запись MIDI-событий"));
	assert!(!is_shown(&buf, "запись аудиопотока"));
	assert_eq!(colour_of(&buf, "запись MIDI-событий"), HINT);
	let (row, col) = find(&buf, "[MIDI]");
	assert_eq!(
		(buf.cell(col, row).fg, buf.cell(col, row).bg),
		(BACKGROUND, VALUE),
		"the chosen format is lit"
	);
	assert_eq!(colour_of(&buf, "[FLAC]"), TEXT);
	assert_eq!(click(&mut buttons, &buf, "[MIDI]"), Touch::Pressed(ButtonId::FormatMidi));
	assert!(is_shown(
		&rendered(&App::new("en"), &midi),
		"[FLAC] [OGG] [MIDI]  records MIDI events"
	));

	// With another format chosen the button is an ordinary one.
	let buf = rendered_ru(&view());
	assert_eq!(colour_of(&buf, "[MIDI]"), TEXT);
	assert!(is_shown(&buf, "[MIDI]  запись аудиопотока"));

	// Under a running MIDI take the format is locked like the others.
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(3));
	let mut buttons = Buttons::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &app, &midi, &mut buttons);
	let (row, col) = find(&buf, "[MIDI]");
	assert_eq!(buf.cell(col, row).bg, VALUE);
	assert_eq!(click(&mut buttons, &buf, "[MIDI]"), Touch::Missed);
	assert_eq!(colour_of(&buf, "[FLAC]"), DISABLED);
}

#[test]
fn a_small_take_is_sized_in_kilobytes_and_a_large_one_in_megabytes() {
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(5));
	let mut recording = view();
	recording.take = TakeView::Recording {
		name: "2026-11-13_12-03-41.mid".into(),
		size: Size::of(3 * 1024 + 100),
	};
	assert!(is_shown(
		&rendered(&app, &recording),
		"● ЗАП 00:05 · 3 КБ · 2026-11-13_12-03-41.mid"
	));

	let mut saved = view();
	saved.take = TakeView::Saved {
		path: "D:/rec/take.mid".into(),
		size: Size::of(700),
	};
	assert!(is_shown(&rendered_ru(&saved), "сохранено: D:/rec/take.mid · 1 КБ"));
	assert!(is_shown(&rendered(&App::new("en"), &saved), "saved: D:/rec/take.mid · 1 KB"));
	saved.take = TakeView::Saved {
		path: "D:/rec/take.flac".into(),
		size: Size::of(1024 * 1024),
	};
	assert!(is_shown(&rendered_ru(&saved), "сохранено: D:/rec/take.flac · 1.0 МБ"));
}
