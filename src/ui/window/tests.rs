//! Tests of the window glue that need no window: the "what the frame shows" key.

use super::*;
use crate::audio::{Levels, Mixer};
use crate::config::RecordFormat;
use crate::midi::MidiStatus;

fn view() -> View {
	View {
		midi: MidiStatus::NoPort,
		audio: None,
		bank: None,
		reverb: None,
		chorus: None,
		record_dir: String::new(),
		record_format: RecordFormat::Flac,
		xruns: 0,
		dropped: 0,
		transpose: 0,
		omni_channel: None,
		record_dropped: 0,
		held_notes: 0,
		max_voices: 64,
		load_percent: 0,
		levels: Levels::default(),
		mixer: Mixer::new(80),
		notice: None,
		keyboard_notes: 0,
		midi_notes: 0,
	}
}

#[test]
fn the_frame_key_changes_only_when_the_picture_would() {
	let grid = (96, 31);
	let app = App::new("en");
	let shown = FrameState::new(view(), &app, grid);
	assert_eq!(FrameState::new(view(), &app, grid), shown, "nothing changed: no redraw");

	let mut louder = view();
	louder.levels.rms_left = 0.25;
	assert_ne!(FrameState::new(louder, &app, grid), shown, "a meter moved");
	assert_ne!(FrameState::new(view(), &app, (120, 40)), shown, "the grid changed");

	let mut clipping = App::new("en");
	clipping.clipping = true;
	assert_ne!(FrameState::new(view(), &clipping, grid), shown);

	let mut higher = App::new("en");
	higher.keyboard.octave_up();
	assert_ne!(FrameState::new(view(), &higher, grid), shown);
	let mut softer = App::new("en");
	softer.keyboard.velocity_down();
	assert_ne!(FrameState::new(view(), &softer, grid), shown);
}

#[test]
fn a_running_take_redraws_once_per_blink_phase_and_per_second() {
	let grid = (96, 31);
	let at = |milliseconds: u64| {
		let mut app = App::new("en");
		app.recording = Some(Duration::from_millis(milliseconds));
		FrameState::new(view(), &app, grid)
	};
	assert_ne!(at(0), FrameState::new(view(), &App::new("en"), grid), "the indicator appears");
	assert_eq!(at(0), at(499), "same second, dot still lit: no redraw");
	assert_ne!(at(499), at(500), "the dot goes dark");
	assert_eq!(at(500), at(999));
	assert_ne!(at(999), at(1_000), "the next second starts and the dot lights up");
	assert!(blink_phase(Duration::from_millis(0)));
	assert!(!blink_phase(Duration::from_millis(500)));
	assert!(blink_phase(Duration::from_millis(1_000)));
}

#[test]
fn a_folder_for_takes_is_anything_but_an_existing_file() {
	let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
	assert_eq!(record_dir_from(Path::new("")), Some(None), "empty: the default folder");
	let assets = repository.join("assets");
	assert_eq!(record_dir_from(&assets), Some(Some(assets.clone())));
	let new_folder = repository.join("target").join("no-such-folder-for-takes");
	assert_eq!(
		record_dir_from(&new_folder),
		Some(Some(new_folder.clone())),
		"a folder that does not exist yet is created when a take starts"
	);
	assert_eq!(record_dir_from(&assets.join("sin_wave.sf2")), None, "a file is not a folder");
}

#[test]
fn a_mouse_event_is_inside_an_area_only_over_its_cells() {
	let area = Rect::new(4, 8, 10, 3);
	let at = |col, row| MouseMsg {
		kind: gterm_widgets::MouseKind::Down,
		button: Some(winit::event::MouseButton::Left),
		col,
		row,
		modifiers: winit::keyboard::ModifiersState::empty(),
	};
	for (col, row) in [(4, 8), (13, 10), (9, 9)] {
		assert!(is_inside(area, &at(col, row)), "{col},{row}");
	}
	for (col, row) in [(3, 8), (14, 8), (4, 7), (4, 11), (-1, -1), (70_000, 9)] {
		assert!(!is_inside(area, &at(col, row)), "{col},{row}");
	}
}
