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
use crate::ui::app::Dropdown;
use crate::ui::chord::{KeyCode, KeyEvent, KeyModifiers};
use crate::ui::controls::Touch;
use crate::ui::testkit::cell_of;
use crate::ui::view::{AudioView, BankView, FileView, Size, TakeView};

/// The grid the screen is drawn on in these tests: the smallest one it fits in.
const GRID: Rect = Rect::new(0, 0, MIN_COLS, MIN_ROWS);

fn view() -> View {
	View {
		midi: MidiStatus::Connected("Arturia KeyStep 37".into()),
		audio: Some(AudioView {
			device: "[WASAPI]: Speakers".into(),
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
	render(&mut buf, app, view, &mut Controls::new());
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

/// The cells of a screen row between the borders of the frame, trailing spaces dropped.
fn inside(line: &str) -> String {
	let cells: Vec<char> = line.chars().collect();
	assert_eq!((cells[0], cells[cells.len() - 1]), ('│', '│'), "{line:?} is a row inside the frame");
	let inner: String = cells[1..cells.len() - 1].iter().collect();
	inner.trim_end().to_owned()
}

/// The top border of a grid `cols` cells wide: the name and the version a cell away from the corners.
fn top_border(cols: usize) -> String {
	let version = env!("CARGO_PKG_VERSION");
	let rule = cols - " sf2_player ".len() - version.len() - 4;
	format!("┌ sf2_player {} {version} ┐", "─".repeat(rule))
}

/// The bottom border of the smallest grid: the counters at its left end, the engine at its right one.
fn bottom_border(counters: &str, engine: &str) -> String {
	let rule = usize::from(MIN_COLS) - counters.chars().count() - engine.chars().count() - 6;
	format!("└ {counters} {} {engine} ┘", "─".repeat(rule))
}

/// The rule of a section as it stands inside the frame of the smallest grid: two cells of inset,
/// `─ `, what the section puts on its rule, and the rule up to the inset on the right.
fn rule_row(head: &str) -> String {
	let cells = usize::from(MIN_COLS) - 6;
	format!("  ─ {head} {}", "─".repeat(cells - head.chars().count() - 3))
}

/// The four rows of the piano with the characters of the default text keyboard.
const PIANO: [&str; 4] = [
	"│  │s│ │d│  │  │g│ │h│ │j│  │  │l│ │;│  │",
	"│  └┬┘ └┬┘  │  └┬┘ └┬┘ └┬┘  │  └┬┘ └┬┘  │",
	"│ z │ x │ c │ v │ b │ n │ m │ , │ . │ / │",
	"└───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘",
];

/// The rows of the screen inside the frame from the rule of "Output" down: they stand on the bottom
/// of the frame and are the same whichever engine is chosen.
fn lower_rows() -> Vec<String> {
	vec![
		rule_row("ВЫХОД"),
		String::new(),
		"  Аудио выход       [WASAPI]: Speakers                               ▾  48000 Hz / 256  OK".to_owned(),
		"  L                 ░░░░░░░░░░░░░░░░░░░░░░░░░░      −∞ dBFS".to_owned(),
		"  R                 ░░░░░░░░░░░░░░░░░░░░░░░░░░      −∞ dBFS".to_owned(),
		"  Мастер            [−] ████████████████░░░░  80% [+]".to_owned(),
		String::new(),
		rule_row("ЗАПИСЬ"),
		String::new(),
		"  Папка для файлов: [D:/Music/sf2_player]".to_owned(),
		"            Формат: FLAC   ▾  запись аудиопотока".to_owned(),
		"            Запись: [Старт] [Стоп] [Отмена]  записей ещё не было".to_owned(),
		String::new(),
	]
}

/// The white keys of the piano that are lit, by their place from the left.
fn lit_white_keys(buf: &ScreenBuffer) -> Vec<usize> {
	let (x, y) = piano_origin(buf.area()).expect("the piano is shown");
	(0..10)
		.filter(|key| buf.cell(usize::from(x) + key * 4 + 2, usize::from(y) + 2).bg == VALUE)
		.collect()
}

/// Foreground colours of the `cells` cells that follow the label column in the row labelled `label`.
fn meter_colours(buf: &ScreenBuffer, label: &str, cells: usize) -> Vec<Color> {
	let (row, _) = find(buf, label);
	(0..cells).map(|cell| buf.cell(usize::from(VALUE_X) + cell, row).fg).collect()
}

#[test]
fn full_frame_matches_the_layout_mockup() {
	let mut expected = vec![
		String::new(),
		rule_row("ИСТОЧНИКИ MIDI"),
		String::new(),
		"  Клавиатура        —                                                 окт 4 vel 100".to_owned(),
		format!("{}{}", " ".repeat(26), PIANO[0]),
		format!("{}{}", " ".repeat(26), PIANO[1]),
		format!("{}{}", " ".repeat(26), PIANO[2]),
		format!("{}{}", " ".repeat(26), PIANO[3]),
		"  MIDI-вход         Arturia KeyStep 37                     ▾  открыто".to_owned(),
		"    → ноты          —".to_owned(),
		"  MIDI-файл         [MIDI-файл]  файл не выбран  [Старт] [Пауза] [Стоп]".to_owned(),
		String::new(),
		String::new(),
		rule_row("ДВИЖОК  [Synth] [Sound Font]"),
		String::new(),
		"  Файл Sound Font   [Файл Sound Font]  GeneralUser GS.sf2  OK".to_owned(),
		"  Пресет            [Пресет]  000:000 Grand Piano".to_owned(),
		"  Эффекты           [Эффекты]  рев 40  хор выкл".to_owned(),
		String::new(),
		String::new(),
		String::new(),
	];
	expected.extend(lower_rows());
	let lines = lines_of(&rendered_ru(&view()));
	let last = lines.len() - 1;
	assert_eq!(lines.len(), usize::from(MIN_ROWS));
	assert_eq!(lines[0], top_border(usize::from(MIN_COLS)));
	assert_eq!(lines[0].chars().count(), usize::from(MIN_COLS));
	let inner: Vec<String> = lines[1..last].iter().map(|line| inside(line)).collect();
	assert_eq!(inner, expected, "\n{}", lines.join("\n"));
	assert_eq!(
		lines[last],
		bottom_border("голоса   7/64   cpu   3%  xrun     0", "движок: SF2 · GeneralUser GS.sf2")
	);
	assert_eq!(lines[last].chars().count(), usize::from(MIN_COLS));
}

#[test]
fn everything_inside_the_frame_keeps_two_cells_from_its_sides_and_a_row_from_its_top_and_bottom() {
	let mut crowded = synth_view();
	crowded.keyboard_notes = sounding(&(0..128).collect::<Vec<u8>>());
	crowded.midi_notes = crowded.keyboard_notes;
	crowded.file = file_view(Transport::Playing).file;
	crowded.file.file = MidiFile::Loaded(format!("D:/midi/{}.mid", "x".repeat(120)).into());
	crowded.record_dir = format!("D:/{}", "папка/".repeat(30));
	crowded.notice = Some(Notice::FontFallback("Comic Mono".repeat(12)));
	crowded.take = TakeView::Recording {
		name: "x".repeat(120),
		size: Size::of(0),
	};
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(5));
	app.clipping = true;
	for (cols, rows) in [(MIN_COLS, MIN_ROWS), (130, 50)] {
		let mut buf = screen(usize::from(cols), usize::from(rows));
		render(&mut buf, &app, &crowded, &mut Controls::new());
		let (cols, rows) = (buf.cols(), buf.rows());
		let is_blank = |col: usize, row: usize| matches!(buf.cell(col, row).ch, '\0' | ' ');
		for row in 1..rows - 1 {
			assert_eq!(
				(buf.cell(0, row).ch, buf.cell(cols - 1, row).ch),
				('│', '│'),
				"the frame in row {row}"
			);
			for col in [1, 2, cols - 3, cols - 2] {
				assert!(is_blank(col, row), "cell {col},{row} of the inset at {cols}×{rows}");
			}
		}
		for col in 1..cols - 1 {
			assert!(is_blank(col, 1), "cell {col},1 of the inset at {cols}×{rows}");
		}
		// The inset row at the bottom is where a message that belongs to no row goes, and nothing else.
		let message = inside(&lines_of(&buf)[rows - 2]);
		assert!(message.starts_with("  Шрифт Comic Mono"), "{message}");
		assert_eq!((buf.cell(0, 0).ch, buf.cell(cols - 1, 0).ch), ('┌', '┐'));
		assert_eq!((buf.cell(0, rows - 1).ch, buf.cell(cols - 1, rows - 1).ch), ('└', '┘'));
	}
}

#[test]
fn a_rule_has_a_blank_row_above_and_below_and_the_last_two_sections_stand_on_the_bottom() {
	for rows in [MIN_ROWS, 50] {
		for engine in [view(), synth_view()] {
			let mut buf = screen(usize::from(MIN_COLS), usize::from(rows));
			render(&mut buf, &App::new("ru"), &engine, &mut Controls::new());
			let lines = lines_of(&buf);
			let rule_of = |title: &str| find(&buf, &format!("─ {title} ")).0;
			let rules = [rule_of("ИСТОЧНИКИ MIDI"), rule_of("ДВИЖОК"), rule_of("ВЫХОД"), rule_of("ЗАПИСЬ")];
			for row in rules {
				assert_eq!(inside(&lines[row - 1]), "", "the row above the rule in row {row}");
				assert_eq!(inside(&lines[row + 1]), "", "the row below the rule in row {row}");
			}
			// The first two hang from the top of the frame, the last two stand on its bottom.
			let bottom = usize::from(rows) - 1;
			assert_eq!(rules, [2, 14, bottom - 13, bottom - 6], "at {rows} rows");
		}
	}
}

#[test]
fn english_screen_uses_the_dictionary_for_every_label() {
	let buf = rendered(&App::new("en"), &view());
	for text in [
		"─ MIDI SOURCES ──",
		" Keyboard ",
		" MIDI input        Arturia KeyStep 37                     ▾  open",
		"   → notes ",
		" MIDI file         [MIDI file]  no file selected  [Start] [Pause] [Stop]",
		"─ ENGINE  [Synth] [Sound Font] ──",
		" Sound Font file   [Sound Font file]  GeneralUser GS.sf2  OK",
		" Preset            [Preset]  000:000 Grand Piano",
		" Effects           [Effects]  rev 40  cho off",
		"─ OUTPUT ──",
		" Audio output      [WASAPI]: Speakers                               ▾  48000 Hz / 256  OK",
		" Master            [−] ",
		"─ RECORDING ──",
		"  Folder for files: [D:/Music/sf2_player]",
		"            Format: FLAC   ▾  records the audio stream",
		"         Recording: [Start] [Stop] [Cancel]  no takes yet",
		"└ voices   7/64   cpu   3%  xrun     0 ──",
		"── engine: SF2 · GeneralUser GS.sf2 ┘",
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
	let (row, col) = find(&buf, "256  OK");
	assert_eq!(buf.cell(col + "256  ".chars().count(), row).fg, GOOD, "OK after the audio device");
	assert_eq!(colour_of(&buf, "48000 Hz"), HINT);
	assert_eq!(colour_of(&buf, "Клавиатура"), LABEL);
	assert_eq!(colour_of(&buf, "ИСТОЧНИКИ MIDI"), LABEL);
	assert_eq!(colour_of(&buf, "─ ИСТОЧНИКИ MIDI"), RULE);
	assert_eq!(colour_of(&buf, "┌"), RULE, "the frame is as dark as the rules");
	assert_eq!(colour_of(&buf, "sf2_player"), LABEL);
	assert_eq!(colour_of(&buf, env!("CARGO_PKG_VERSION")), LABEL);
	assert_eq!(colour_of(&buf, "голоса"), LABEL);
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
			"Запись: ",
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
	assert!(!is_shown(&rendered_ru(&broken), "256  OK"));
	broken.notice = Some(Notice::Sf2Error("not a RIFF file".into()));
	assert!(!is_shown(&rendered_ru(&broken), "GeneralUser GS.sf2  OK"));
	assert!(is_shown(&rendered_ru(&broken), "256  OK"), "the audio line is not affected");
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
	assert_eq!(colour_of(&buf, "[Synth]"), TEXT, "the other engine can be chosen");
	let (row, col) = find(&buf, "[Sound Font]");
	let chosen = buf.cell(col, row);
	assert_eq!(
		(chosen.fg, chosen.bg),
		(BACKGROUND, VALUE),
		"the chosen engine is inverted on amber"
	);
	assert!(chosen.flags.contains(CellFlags::BOLD));
	assert_eq!(colour_of(&buf, "[MIDI-файл]"), TEXT, "a file can be chosen");
	assert_eq!(colour_of(&buf, "[Пауза]"), DISABLED);
	assert_eq!(colour_of(&buf, "Arturia KeyStep 37"), TEXT, "the port in use");
	// The field of the format dropdown names the format in use.
	assert!(is_shown(&buf, "Формат: FLAC   ▾"));
	assert_eq!(colour_of(&buf, "FLAC   ▾"), TEXT);

	let mut vorbis = view();
	vorbis.record_format = RecordFormat::OggVorbis;
	let buf = rendered_ru(&vorbis);
	assert!(is_shown(&buf, "Формат: OGG    ▾"));
	assert!(!is_shown(&buf, "FLAC"));
}

#[test]
fn recording_shows_a_blinking_red_indicator_and_swaps_the_available_buttons() {
	let idle = rendered_ru(&view());
	let (row, _) = find(&idle, " Запись:");
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
		// What follows the corner of the bottom border.
		line.chars().skip(1).take(38).collect::<String>()
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
	// The counters win: the engine summary gives way when the border is full.
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
	let (record_row, _) = find(&buf, " Запись:");
	assert_eq!(row, record_row + 1, "the message line sits under the recording row");
	assert_eq!(row, usize::from(MIN_ROWS) - 2, "in the inset row over the bottom of the frame");
	assert_eq!(buf.cell(usize::from(LABEL_X), row).fg, MESSAGE);

	// A message too long for the row is cut and marked at the inset of the frame.
	let mut long = view();
	long.notice = Some(Notice::FontFallback("Comic Mono ".repeat(12)));
	let buf = rendered_ru(&long);
	let (row, _) = find(&buf, "Шрифт Comic Mono");
	let line = inside(&lines_of(&buf)[row]);
	assert!(line.ends_with('…'), "{line}");
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 4);

	let mut font = view();
	font.notice = Some(Notice::FontFallback("Comic Mono".into()));
	assert!(is_shown(&rendered_ru(&font), "Шрифт Comic Mono недоступен"));
	assert!(is_shown(&rendered(&App::new("en"), &font), "Font Comic Mono is unavailable"));
}

#[test]
fn a_grid_below_the_minimum_shows_only_the_hint() {
	for (cols, rows) in [(MIN_COLS - 1, MIN_ROWS), (MIN_COLS, MIN_ROWS - 1), (20, 5)] {
		let mut buf = screen(usize::from(cols), usize::from(rows));
		render(&mut buf, &App::new("ru"), &view(), &mut Controls::new());
		let lines = lines_of(&buf);
		let hint: String = "Увеличьте окно до 96×36".chars().take(usize::from(cols)).collect();
		assert_eq!(lines[0], hint);
		assert_eq!(buf.cell(0, 0).fg, ERROR);
		assert!(lines[1..].iter().all(String::is_empty), "nothing else is drawn at {cols}×{rows}");
	}

	// A larger grid keeps the layout: the piano stays centred, the status goes to the last row.
	let mut wide = screen(120, 40);
	render(&mut wide, &App::new("ru"), &view(), &mut Controls::new());
	let (piano_row, piano_col) = find(&wide, PIANO[0]);
	assert_eq!(piano_col, (120 - usize::from(piano::WIDTH)) / 2);
	assert_eq!(
		piano_origin(wide.area()),
		Some((piano_col as u16, piano_row as u16)),
		"the mouse looks for the piano where it is drawn"
	);
	let (status_row, _) = find(&wide, "голоса");
	assert_eq!(status_row, 39);
	assert_eq!(lines_of(&wide)[0], top_border(120), "the frame spans the whole width");
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
	let mut buttons = Controls::new();
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
fn click(buttons: &mut Controls, buf: &ScreenBuffer, text: &str) -> Touch {
	let (row, col) = find(buf, text);
	let at = |kind| MouseMsg {
		kind,
		button: Some(MouseButton::Left),
		col: i32::try_from(col).expect("column"),
		row: i32::try_from(row).expect("row"),
		modifiers: ModifiersState::empty(),
		click_count: 1,
	};
	buttons.handle_mouse(&at(MouseKind::Down), GRID);
	buttons.handle_mouse(&at(MouseKind::Up), GRID)
}

/// Tab to the next button: a dropdown on the way is passed by.
fn next_button(buttons: &mut Controls) {
	let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
	buttons.handle_key(&tab);
	while buttons.focused().is_none() && buttons.focused_dropdown().is_some() {
		buttons.handle_key(&tab);
	}
}

/// The buttons Tab stops at, starting from the one under the focus, once round the ring.
fn ring(buttons: &mut Controls) -> Vec<ButtonId> {
	let mut stops = Vec::new();
	while let Some(focused) = buttons.focused().filter(|focused| !stops.contains(focused)) {
		stops.push(focused);
		next_button(buttons);
	}
	stops
}

#[test]
fn every_button_of_the_screen_is_pressed_by_a_click_and_by_tab_and_enter() {
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	// The text of a button as it stands on the screen, with enough around it to be unique.
	let available = [
		(ButtonId::MidiFile, "[MIDI-файл]"),
		(ButtonId::EngineSynth, "[Synth]"),
		(ButtonId::EngineSf2, "[Sound Font]"),
		(ButtonId::Sf2File, "[Файл Sound Font]"),
		(ButtonId::Preset, "[Пресет]"),
		(ButtonId::Effects, "[Эффекты]"),
		(ButtonId::MasterDown, "[−]"),
		(ButtonId::MasterUp, "[+]"),
		(ButtonId::Folder, "[D:/Music/sf2_player]"),
		(ButtonId::RecordStart, "[Старт] [Стоп] [Отмена]"),
	];
	for (id, text) in available {
		assert_eq!(click(&mut buttons, &buf, text), Touch::Pressed(id), "{text}");
		assert_eq!(buttons.focused(), Some(id), "a click on {text} moves the focus there");
	}

	// Tab goes through the same buttons top to bottom, left to right; Enter presses each.
	click(&mut buttons, &buf, "[MIDI-файл]");
	let order: Vec<ButtonId> = available.iter().map(|(id, _)| *id).collect();
	assert_eq!(ring(&mut buttons), order);
	for id in order {
		assert_eq!(
			buttons.handle_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
			Touch::Pressed(id)
		);
		next_button(&mut buttons);
	}

	// What the screen shows unavailable does not react.
	for text in ["[Старт] [Пауза] [Стоп]", "[Пауза]", "[Стоп]", "[Стоп] [Отмена]", "[Отмена]"] {
		assert_eq!(click(&mut buttons, &buf, text), Touch::Missed, "{text}");
	}
}

#[test]
fn the_control_under_the_focus_is_underlined_from_the_first_frame() {
	let buf = rendered_ru(&view());
	let is_underlined = |text: &str| {
		let (row, col) = find(&buf, text);
		buf.cell(col, row).flags.contains(CellFlags::UNDERLINE)
	};
	assert!(
		is_underlined("Arturia KeyStep 37"),
		"the first control of the screen is the MIDI input"
	);
	assert!(!is_underlined("[Файл Sound Font]") && !is_underlined("FLAC   ▾") && !is_underlined("[MIDI-файл]"));
}

#[test]
fn a_running_take_locks_the_format_and_takes_start_away() {
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(3));
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &app, &view(), &mut buttons);
	// The field still names the format of the take, dark, and does not react.
	assert_eq!(colour_of(&buf, "FLAC   ▾"), DISABLED);
	for text in ["FLAC   ▾", "[Старт] [Стоп] [Отмена]"] {
		assert_eq!(click(&mut buttons, &buf, text), Touch::Missed, "{text}");
	}
	assert_eq!(buttons.open_dropdown(), None);
	assert!(!buttons.open(Dropdown::Format), "nor does anything else open it");
	assert_eq!(click(&mut buttons, &buf, "[Стоп] [Отмена]"), Touch::Pressed(ButtonId::RecordStop));
	let stops = ring(&mut buttons);
	assert!(stops.contains(&ButtonId::RecordStop));
	assert!(!stops.contains(&ButtonId::RecordStart));

	// Without a take the field reacts.
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	assert_eq!(colour_of(&buf, "FLAC   ▾"), TEXT);
	assert!(buttons.open(Dropdown::Format));
}

#[test]
fn the_format_is_chosen_from_a_dropdown_whose_list_opens_upwards() {
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	let (row, col) = find(&buf, "FLAC   ▾");
	assert_eq!(col, usize::from(VALUE_X), "the field stands in the value column");
	let at = |kind, col: i32, row: i32| MouseMsg {
		kind,
		button: Some(MouseButton::Left),
		col,
		row,
		modifiers: ModifiersState::empty(),
		click_count: 1,
	};
	let (field_col, field_row) = (i32::try_from(col).expect("column"), i32::try_from(row).expect("row"));
	assert_eq!(
		buttons.handle_mouse(&at(MouseKind::Down, field_col, field_row), GRID),
		Touch::Opened(Dropdown::Format)
	);
	buttons.handle_mouse(&at(MouseKind::Up, field_col, field_row), GRID);
	buttons.set_items(Dropdown::Format, ["FLAC", "OGG", "MIDI"].map(str::to_owned).to_vec());
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	assert!(is_shown(&buf, "Формат: FLAC   ▴"), "the mark of the field turns");

	// Three rows in a frame do not fit between the field and the bottom of the grid: the list goes up.
	let (area, list) = buttons.layer(GRID).expect("the list is open");
	let field_row = u16::try_from(row).expect("row");
	assert_eq!(area, Rect::new(VALUE_X, field_row - 5, FORMAT_CELLS, 5));
	let rows = lines_of(&list);
	assert!(rows[1].contains("FLAC") && rows[2].contains("OGG") && rows[3].contains("MIDI"));

	let (midi_col, midi_row) = cell_of(area, &list, "MIDI");
	assert_eq!(
		buttons.handle_mouse(&at(MouseKind::Down, midi_col, midi_row), GRID),
		Touch::Chosen(Dropdown::Format, 2)
	);
	assert_eq!(buttons.open_dropdown(), None);
}

#[test]
fn the_folder_button_names_the_folder_and_a_long_path_loses_its_beginning() {
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	assert!(is_shown(&buf, "  Папка для файлов: [D:/Music/sf2_player]"));
	assert_eq!(colour_of(&buf, "Папка для файлов:"), LABEL);
	assert_eq!(colour_of(&buf, "[D:/Music/sf2_player]"), TEXT);
	assert_eq!(click(&mut buttons, &buf, "[D:/Music/sf2_player]"), Touch::Pressed(ButtonId::Folder));

	let mut deep = view();
	deep.record_dir = format!("D:/{}записи", "папка/".repeat(20));
	render(&mut buf, &App::new("ru"), &deep, &mut buttons);
	let (row, col) = find(&buf, "[…");
	let line = inside(&lines_of(&buf)[row]);
	assert_eq!(col, usize::from(VALUE_X));
	assert!(line.ends_with("папка/папка/записи]"), "{line}");
	assert_eq!(
		line.chars().count(),
		usize::from(MIN_COLS) - 4,
		"the button ends where the content does"
	);
	assert_eq!(click(&mut buttons, &buf, "[…"), Touch::Pressed(ButtonId::Folder));
}

#[test]
fn on_a_grid_below_the_minimum_no_button_reacts() {
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &view(), &mut buttons);
	assert_eq!(buttons.focused_dropdown(), Some(Dropdown::MidiIn));
	let mut small = screen(usize::from(MIN_COLS) - 1, usize::from(MIN_ROWS));
	render(&mut small, &App::new("ru"), &view(), &mut buttons);
	assert_eq!(buttons.focused(), None);
	assert_eq!(buttons.focused_dropdown(), None);
	assert_eq!(
		buttons.handle_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
		Touch::Missed
	);
	// Where `[MIDI-файл]` stood on the full screen.
	assert_eq!(click(&mut buttons, &buf, "[MIDI-файл]"), Touch::Missed);
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
	assert_eq!(lit_white_keys(&buf), [0, 1, 2, 4], "C D E G");
	let (row, col) = piano_origin(buf.area())
		.map(|(x, y)| (usize::from(y), usize::from(x)))
		.expect("shown");
	let sharp = buf.cell(col + 4, row);
	assert_eq!(
		(sharp.ch, sharp.fg, sharp.bg),
		('s', BACKGROUND, VALUE),
		"C# is lit at its character"
	);
	let other = buf.cell(col + 8, row);
	assert_eq!((other.ch, other.fg, other.bg), ('d', LABEL, BACKGROUND), "D# is not");
	// A lit key keeps its character.
	for row in PIANO {
		assert!(is_shown(&buf, row), "{row}");
	}

	// Silence: a dash in both rows, a grey piano.
	let buf = rendered_ru(&view());
	assert!(is_shown(&buf, " Клавиатура        —"));
	assert!(is_shown(&buf, "   → ноты          —"));
	assert!(lit_white_keys(&buf).is_empty());
	assert_eq!(colour_of(&buf, PIANO[2]), HINT, "the lines of the piano are dark");
}

#[test]
fn the_piano_follows_the_base_octave_of_the_text_keyboard() {
	let mut playing = view();
	playing.midi_notes = sounding(&[60]);
	let mut app = App::new("ru");
	assert_eq!(lit_white_keys(&rendered(&app, &playing)), [0], "C4 is the first key");
	app.keyboard.octave_down();
	assert_eq!(lit_white_keys(&rendered(&app, &playing)), [7], "from C3 it is the eighth");
	app.keyboard.octave_up();
	app.keyboard.octave_up();
	assert!(lit_white_keys(&rendered(&app, &playing)).is_empty(), "from C5 it is out of range");
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
	// The MIDI row has the whole width up to the inset of the frame: 72 cells.
	let (midi_row, _) = find(&buf, "   → ноты ");
	let names_of = |buf: &ScreenBuffer| -> String { inside(&lines_of(buf)[midi_row]).chars().skip(usize::from(VALUE_X) - 1).collect() };
	assert_eq!(
		names_of(&buf),
		"C2 C#2 D2 D#2 E2 F2 F#2 G2 G#2 A2 A#2 B2 C3 C#3 D3 D#3 E3 F3 F#3 G3 …"
	);

	// Twenty-five natural notes from C2 take 74 cells — two more than the row has: the list is cut.
	let naturals: Vec<u8> = (36..128)
		.filter(|key| [0, 2, 4, 5, 7, 9, 11].contains(&(key % 12)))
		.take(25)
		.collect();
	playing.midi_notes = sounding(&naturals);
	let midi = names_of(&rendered_ru(&playing));
	assert!(midi.ends_with("D5 …") && !midi.contains("E5"), "{midi}");
	assert!(midi.chars().count() <= 72);
	// One note fewer takes 71 cells and fits whole.
	playing.midi_notes = sounding(&naturals[..24]);
	let midi = names_of(&rendered_ru(&playing));
	assert!(midi.ends_with("D5 E5"), "{midi}");
	assert_eq!(midi.chars().count(), 71);
}

/// The view with the built-in synthesizer chosen.
fn synth_view() -> View {
	let mut view = view();
	view.engine = EngineKind::Builtin;
	view
}

#[test]
fn the_synth_screen_matches_the_layout_mockup() {
	let mut expected = vec![
		rule_row("ДВИЖОК  [Synth] [Sound Font]"),
		String::new(),
		"  Форма сигнала     [Синусоида] [Пила] [Треугольник]".to_owned(),
		"  ADSR              пресет: [Пианино] [Клавесин] [Орган] [Флейта]".to_owned(),
		"    значения        Attack: [−] 0.003 с [+]   Decay: [−] 0.500 с [+]".to_owned(),
		"                    Sustain: [−]  35% [+]     Release: [−] 0.350 с [+]".to_owned(),
		"  Эффекты           [Эффекты]  рев 40  хор выкл".to_owned(),
		String::new(),
	];
	expected.extend(lower_rows());
	let buf = rendered_ru(&synth_view());
	let lines = lines_of(&buf);
	let last = lines.len() - 1;
	// The sections above the engine are the same for both; the synthesizer takes two rows more, the
	// sections under it stay where they are, and the screen still fits the smallest grid.
	assert_eq!(lines[..14], lines_of(&rendered_ru(&view()))[..14]);
	let inner: Vec<String> = lines[14..last].iter().map(|line| inside(line)).collect();
	assert_eq!(inner, expected, "{}", lines[14..].join("|"));
	assert_eq!(
		lines[last],
		bottom_border("голоса   7/64   cpu   3%  xrun     0", "движок: Synth · Синусоида")
	);
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
	let mut buttons = Controls::new();
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
		buttons.handle_mouse(&at(MouseKind::Down), GRID);
		assert_eq!(buttons.handle_mouse(&at(MouseKind::Up), GRID), Touch::Pressed(id));
	}

	// Tab walks them between the MIDI input and the output, in the order they stand on the screen;
	// the buttons of the Sound Font are not in the ring.
	click(&mut buttons, &buf, "[MIDI-файл]");
	let stops = ring(&mut buttons);
	assert_eq!(
		stops[..19],
		[
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
		"[MIDI-файл]  MIDI file format 2 is not supporte…  [Старт] [Пауза] [Стоп]"
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
	let line = inside(&lines_of(&buf)[row]);
	assert_eq!(
		line,
		"  MIDI-файл         [MIDI-файл]  Johann Sebastian Bach - Das Wohlte…  [Старт] [Пауза] [Стоп]"
	);
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 4, "up to the inset of the frame");
	// The buttons are still where a click finds them.
	let mut buttons = Controls::new();
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
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &App::new("ru"), &file_view(Transport::Paused), &mut buttons);
	click(&mut buttons, &buf, "[MIDI-файл]");
	let stops = ring(&mut buttons);
	assert_eq!(
		stops[..4],
		[ButtonId::MidiFile, ButtonId::FileStart, ButtonId::FileStop, ButtonId::EngineSynth]
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
	let mut buttons = Controls::new();
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
	let line = inside(&lines_of(&buf)[row]);
	assert!(line.ends_with("xx…"));
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 4, "up to the inset of the frame");

	// Without a take there is nothing to cancel.
	let idle = rendered_ru(&view());
	assert_eq!(colour_of(&idle, "[Отмена]"), DISABLED);
}

#[test]
fn the_last_saved_take_is_shown_in_green_with_its_path_and_size() {
	let mut saved = view();
	saved.take = TakeView::Saved {
		path: "D:/2026-11-13_12-03-41.flac".into(),
		size: Size::of(5 * 1024 * 1024 + 300 * 1024),
	};
	let buf = rendered_ru(&saved);
	// The longest that fits whole: it ends where the content does.
	assert!(is_shown(&buf, "[Отмена]  сохранено: D:/2026-11-13_12-03-41.flac · 5.3 МБ  │"));
	assert_eq!(colour_of(&buf, "сохранено:"), GOOD);
	assert!(!is_shown(&buf, "записей ещё не было"));
	assert!(is_shown(
		&rendered(&App::new("en"), &saved),
		"[Cancel]  saved: D:/2026-11-13_12-03-41.flac · 5.3 MB"
	));

	// A long path loses its beginning, not the file name; the row stops at the inset of the frame.
	saved.take = TakeView::Saved {
		path: format!("D:/{}2026-11-13_12-03-41.flac", "папка/".repeat(20)),
		size: Size::TenthsOfMegabyte(12),
	};
	let buf = rendered_ru(&saved);
	let (row, _) = find(&buf, "сохранено: …");
	let line = inside(&lines_of(&buf)[row]);
	assert!(line.ends_with("сохранено: …а/2026-11-13_12-03-41.flac · 1.2 МБ"), "{line}");
	assert_eq!(line.chars().count(), usize::from(MIN_COLS) - 4);

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
fn the_format_row_says_what_the_chosen_format_records() {
	let mut midi = view();
	midi.record_format = RecordFormat::Midi;
	let buf = rendered_ru(&midi);
	assert!(is_shown(&buf, "Формат: MIDI   ▾  запись MIDI-событий"));
	assert!(!is_shown(&buf, "запись аудиопотока"));
	assert_eq!(colour_of(&buf, "запись MIDI-событий"), HINT);
	assert_eq!(colour_of(&buf, "MIDI   ▾"), TEXT);
	assert!(is_shown(&rendered(&App::new("en"), &midi), "Format: MIDI   ▾  records MIDI events"));

	// The audio formats record the stream.
	for format in [RecordFormat::Flac, RecordFormat::OggVorbis] {
		let mut audio = view();
		audio.record_format = format;
		assert!(is_shown(&rendered_ru(&audio), "▾  запись аудиопотока"), "{format:?}");
	}

	// Under a running MIDI take the format is locked like the others.
	let mut app = App::new("ru");
	app.recording = Some(Duration::from_secs(3));
	let mut buttons = Controls::new();
	let mut buf = screen(usize::from(MIN_COLS), usize::from(MIN_ROWS));
	render(&mut buf, &app, &midi, &mut buttons);
	assert_eq!(colour_of(&buf, "MIDI   ▾"), DISABLED);
	assert_eq!(click(&mut buttons, &buf, "MIDI   ▾"), Touch::Missed);
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

/// The audio output row as the screen shows it.
fn output_row(view: &View) -> String {
	let buf = rendered_ru(view);
	let (row, _) = find(&buf, "Аудио выход");
	lines_of(&buf)[row].clone()
}

fn with_output(device: &str) -> View {
	let mut view = view();
	view.audio.as_mut().expect("the view has an output").device = device.into();
	view
}

#[test]
fn the_output_field_has_room_for_an_exclusive_output_and_what_follows_it() {
	let row = output_row(&with_output("[WASAPI Exclusive]: Динамики (Realtek(R) Audio)"));
	assert!(
		row.contains("[WASAPI Exclusive]: Динамики (Realtek(R) Audio)  ▾  48000 Hz / 256  OK"),
		"{row:?}"
	);
}

#[test]
fn an_output_name_longer_than_the_field_loses_its_end() {
	let row = output_row(&with_output("[WASAPI Exclusive]: Головной телефон (QCY H3 Hands-Free AG Audio)"));
	assert!(
		row.contains("[WASAPI Exclusive]: Головной телефон (QCY H3 Ha… ▾  48000 Hz / 256  OK"),
		"{row:?}"
	);
}

#[test]
fn the_output_row_names_the_rate_alone_when_the_driver_does_not_tell_the_size_of_a_callback() {
	let mut silent = view();
	silent.audio.as_mut().expect("the view has an output").buffer = None;
	assert!(output_row(&silent).contains("▾  48000 Hz  OK"));
}

#[test]
fn an_error_of_the_output_stays_inside_the_frame() {
	let mut broken = view();
	broken.notice = Some(Notice::AudioError(
		"cannot open audio stream on [WASAPI Exclusive]: Speakers: the device is in use".into(),
	));
	let row = output_row(&broken);
	assert_eq!(row.chars().count(), usize::from(MIN_COLS), "{row:?}");
	assert!(row.ends_with("…  │"), "the error is cut before the inset of the frame: {row:?}");
}
