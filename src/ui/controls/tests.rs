//! Tests of the main screen controls: placed in a `ScreenBuffer`, pressed and chosen through the same
//! entry points the window uses. No window.

use gterm::cell::CellFlags;
use gterm_widgets::MouseKind;
use winit::event::MouseButton;

use super::*;
use crate::ui::testkit::{cell_of, colours_of, left_click, lines_of};

/// The grid of these tests: room for a dropdown list under the rows the frames use.
const GRID: Rect = Rect::new(0, 0, 40, 14);
/// The marks of a dropdown field: closed and open.
const CLOSED: char = '▾';
const OPEN: char = '▴';

fn screen() -> ScreenBuffer {
	ScreenBuffer::with_default_colors(usize::from(GRID.width), usize::from(GRID.height), TEXT, BACKGROUND)
}

/// A frame with four buttons in a row: two that react, one unavailable, one more that reacts.
/// Columns: `[In]` 0..4, `[Flac]` 5..11, `[Midi]` 12..18, `[Stop]` 19..25.
/// The buffer is cleared first, as the screen does: a style is laid over what a cell already has.
fn frame(controls: &mut Controls, buf: &mut ScreenBuffer) {
	buf.clear_all();
	controls.begin_frame();
	let mut x = 0;
	for (id, label, look) in [
		(ButtonId::MidiFile, "In", Look::Normal),
		(ButtonId::EngineSf2, "Flac", Look::Active),
		(ButtonId::Preset, "Midi", Look::Disabled),
		(ButtonId::RecordStop, "Stop", Look::Normal),
	] {
		x += controls.place(id, x, 1, label, look) + 1;
	}
	controls.finish_frame(buf);
}

fn drawn() -> (Controls, ScreenBuffer) {
	let mut controls = Controls::new();
	let mut buf = screen();
	frame(&mut controls, &mut buf);
	(controls, buf)
}

fn key(code: KeyCode) -> KeyEvent {
	KeyEvent::new(code, KeyModifiers::NONE)
}

fn mouse(kind: MouseKind, col: i32, row: i32) -> MouseMsg {
	MouseMsg {
		kind,
		button: matches!(kind, MouseKind::Down | MouseKind::Up).then_some(MouseButton::Left),
		col,
		row,
		modifiers: ModifiersState::empty(),
		click_count: u8::from(matches!(kind, MouseKind::Down | MouseKind::Up)),
	}
}

fn text_at(buf: &ScreenBuffer, col: usize, row: usize, cells: usize) -> String {
	(col..col + cells).map(|col| buf.cell(col, row).ch).collect()
}

#[test]
fn a_button_takes_the_cells_of_its_label_in_brackets() {
	let mut controls = Controls::new();
	controls.begin_frame();
	assert_eq!(controls.place(ButtonId::MidiFile, 3, 2, "MIDI-файл", Look::Normal), 11);
	let mut buf = screen();
	controls.finish_frame(&mut buf);
	assert_eq!(text_at(&buf, 3, 2, 11), "[MIDI-файл]");
	assert_eq!(buf.cell(2, 2).ch, char::MIN, "nothing is drawn around the button");
	assert_eq!(buf.cell(14, 2).ch, char::MIN);
}

#[test]
fn looks_are_drawn_as_the_spec_describes_them() {
	let (_, buf) = drawn();
	// Normal, and under the focus (the first control that reacts has it from the start): underlined.
	let normal = buf.cell(0, 1);
	assert_eq!((normal.fg, normal.bg), (TEXT, BACKGROUND));
	assert!(normal.flags.contains(CellFlags::UNDERLINE));
	// Active: inverted on amber, bold, not underlined without the focus.
	let active = buf.cell(5, 1);
	assert_eq!((active.fg, active.bg), (BACKGROUND, VALUE));
	assert!(active.flags.contains(CellFlags::BOLD) && !active.flags.contains(CellFlags::UNDERLINE));
	// Unavailable: dark.
	assert_eq!((buf.cell(12, 1).fg, buf.cell(12, 1).bg), (DISABLED, BACKGROUND));
	// The last one: normal without the focus.
	assert!(!buf.cell(19, 1).flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn a_dangerous_button_is_red_and_reacts() {
	let mut controls = Controls::new();
	controls.begin_frame();
	controls.place(ButtonId::RecordCancel, 0, 0, "Cancel", Look::Danger);
	let mut buf = screen();
	controls.finish_frame(&mut buf);
	assert_eq!((buf.cell(0, 0).fg, buf.cell(0, 0).bg), (ERROR, BACKGROUND));
	assert_eq!(controls.focused(), Some(ButtonId::RecordCancel));
	controls.handle_mouse(&mouse(MouseKind::Down, 1, 0), GRID);
	assert_eq!(
		controls.handle_mouse(&mouse(MouseKind::Up, 1, 0), GRID),
		Touch::Pressed(ButtonId::RecordCancel)
	);
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::RecordCancel));
}

#[test]
fn tab_walks_the_buttons_that_react_in_a_ring_and_shift_tab_walks_back() {
	let (mut controls, mut buf) = drawn();
	assert_eq!(controls.focused(), Some(ButtonId::MidiFile));
	let mut walked = Vec::new();
	for _ in 0..4 {
		assert_eq!(controls.handle_key(&key(KeyCode::Tab)), Touch::Changed);
		walked.push(controls.focused().expect("a button has the focus"));
	}
	assert_eq!(
		walked,
		[ButtonId::EngineSf2, ButtonId::RecordStop, ButtonId::MidiFile, ButtonId::EngineSf2],
		"the unavailable button is skipped"
	);
	assert_eq!(controls.handle_key(&key(KeyCode::BackTab)), Touch::Changed);
	assert_eq!(controls.focused(), Some(ButtonId::MidiFile));
	assert_eq!(controls.handle_key(&key(KeyCode::BackTab)), Touch::Changed);
	assert_eq!(controls.focused(), Some(ButtonId::RecordStop), "back from the first is the last");

	// The next frame underlines the button the focus moved to.
	frame(&mut controls, &mut buf);
	assert!(buf.cell(19, 1).flags.contains(CellFlags::UNDERLINE));
	assert!(!buf.cell(0, 1).flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn enter_presses_the_button_under_the_focus_and_other_keys_are_not_for_buttons() {
	let (mut controls, _) = drawn();
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::MidiFile));
	controls.handle_key(&key(KeyCode::Tab));
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::EngineSf2));

	for code in [
		KeyCode::Char(' '),
		KeyCode::Char('z'),
		KeyCode::Esc,
		KeyCode::Up,
		KeyCode::Down,
		KeyCode::Home,
		KeyCode::F(2),
	] {
		assert_eq!(controls.handle_key(&key(code)), Touch::Missed, "{code:?}");
	}
	let mut release = key(KeyCode::Enter);
	release.kind = KeyEventKind::Release;
	assert_eq!(controls.handle_key(&release), Touch::Missed, "letting go of Enter presses nothing");
	for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
		let chord = KeyEvent::new(KeyCode::Enter, modifiers);
		assert_eq!(controls.handle_key(&chord), Touch::Missed, "{modifiers:?}+Enter");
		let chord = KeyEvent::new(KeyCode::Tab, modifiers);
		assert_eq!(controls.handle_key(&chord), Touch::Missed, "{modifiers:?}+Tab");
	}
	assert_eq!(controls.focused(), Some(ButtonId::EngineSf2), "none of that moved the focus");
}

#[test]
fn a_click_presses_on_release_takes_the_focus_and_shows_the_button_held() {
	let (mut controls, mut buf) = drawn();
	// Any cell of the button will do: the last one of `[Stop]`.
	assert_eq!(controls.handle_mouse(&mouse(MouseKind::Down, 24, 1), GRID), Touch::Changed);
	assert_eq!(controls.held(), Some(ButtonId::RecordStop));
	assert_eq!(controls.focused(), Some(ButtonId::RecordStop), "a click moves the focus");
	frame(&mut controls, &mut buf);
	assert_eq!(
		(buf.cell(19, 1).fg, buf.cell(19, 1).bg),
		(BACKGROUND, TEXT),
		"held down: inverted on light"
	);
	assert_eq!(
		controls.handle_mouse(&mouse(MouseKind::Up, 24, 1), GRID),
		Touch::Pressed(ButtonId::RecordStop)
	);
	assert_eq!(controls.held(), None);
	frame(&mut controls, &mut buf);
	assert_eq!((buf.cell(19, 1).fg, buf.cell(19, 1).bg), (TEXT, BACKGROUND));
}

#[test]
fn a_click_is_called_off_by_releasing_elsewhere_or_leaving_the_window() {
	let (mut controls, _) = drawn();
	controls.handle_mouse(&mouse(MouseKind::Down, 0, 1), GRID);
	assert_eq!(controls.handle_mouse(&mouse(MouseKind::Move, 30, 3), GRID), Touch::Changed);
	assert_eq!(
		controls.handle_mouse(&mouse(MouseKind::Up, 30, 3), GRID),
		Touch::Changed,
		"released away from the button"
	);
	assert_eq!(controls.held(), None);

	controls.handle_mouse(&mouse(MouseKind::Down, 0, 1), GRID);
	assert_eq!(controls.handle_mouse(&mouse(MouseKind::Cancel, -1, -1), GRID), Touch::Changed);
	assert_eq!(controls.held(), None);
	assert_eq!(
		controls.handle_mouse(&mouse(MouseKind::Up, 0, 1), GRID),
		Touch::Missed,
		"the release after leaving presses nothing"
	);
}

#[test]
fn the_mouse_away_from_buttons_and_on_an_unavailable_one_does_nothing() {
	let (mut controls, _) = drawn();
	for (col, row) in [(4, 1), (0, 0), (0, 2), (30, 1), (-1, -1)] {
		for kind in [MouseKind::Move, MouseKind::Down, MouseKind::Up] {
			assert_eq!(
				controls.handle_mouse(&mouse(kind, col, row), GRID),
				Touch::Missed,
				"{kind:?} at {col},{row}"
			);
		}
	}
	// `[Midi]` is unavailable.
	for kind in [MouseKind::Down, MouseKind::Up] {
		assert_eq!(controls.handle_mouse(&mouse(kind, 13, 1), GRID), Touch::Missed);
	}
	assert_eq!(controls.focused(), Some(ButtonId::MidiFile), "the focus stays");
	// Moving over a button that reacts changes nothing either: there is no hover look.
	assert_eq!(controls.handle_mouse(&mouse(MouseKind::Move, 6, 1), GRID), Touch::Missed);
	// The right button is not a click.
	let mut right = mouse(MouseKind::Down, 0, 1);
	right.button = Some(MouseButton::Right);
	assert_eq!(controls.handle_mouse(&right, GRID), Touch::Missed);
}

#[test]
fn before_the_first_frame_and_without_placed_controls_nothing_reacts() {
	let mut controls = Controls::new();
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Missed);
	assert_eq!(controls.handle_mouse(&mouse(MouseKind::Down, 0, 1), GRID), Touch::Missed);
	assert!(
		!controls.open(Dropdown::MidiIn),
		"a dropdown that is not on the screen does not open"
	);

	// A frame that places nothing (the window is too small) switches every control off.
	let (mut controls, mut buf) = drawn();
	controls.begin_frame();
	controls.finish_frame(&mut buf);
	assert_eq!(controls.focused(), None);
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Missed);
	assert_eq!(controls.handle_mouse(&mouse(MouseKind::Down, 0, 1), GRID), Touch::Missed);
}

#[test]
fn the_focus_leaves_a_button_that_stopped_reacting_for_the_next_one_that_does() {
	let mut controls = Controls::new();
	let mut buf = screen();
	let mut take = |controls: &mut Controls, is_recording: bool| {
		let unless = |unavailable: bool| if unavailable { Look::Disabled } else { Look::Normal };
		controls.begin_frame();
		controls.place(ButtonId::MidiFile, 0, 0, "In", Look::Normal);
		controls.place(ButtonId::RecordStart, 0, 1, "Start", unless(is_recording));
		controls.place(ButtonId::RecordStop, 8, 1, "Stop", unless(!is_recording));
		controls.finish_frame(&mut buf);
	};
	take(&mut controls, false);
	controls.handle_key(&key(KeyCode::Tab));
	assert_eq!(controls.focused(), Some(ButtonId::RecordStart));
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::RecordStart));

	// The take runs: `[Start]` is unavailable, the focus is on `[Stop]`, Enter stops.
	take(&mut controls, true);
	assert_eq!(controls.focused(), Some(ButtonId::RecordStop));
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::RecordStop));

	// Stopped: from `[Stop]`, the last button, the ring goes round to the first.
	take(&mut controls, false);
	assert_eq!(controls.focused(), Some(ButtonId::MidiFile));
}

// --- the device dropdowns (spec v1.3, F-26.1) --------------------------------------------------

const PORTS: [&str; 3] = ["KeyStep", "loopMIDI", "Roland"];

/// A frame with the two dropdowns and a button between them, as they follow each other on the
/// screen. Row 0: the MIDI field, columns 0..20. Row 1: `[Preset]` at 0..8. Row 2: the audio field,
/// columns 0..20.
fn device_frame(controls: &mut Controls, buf: &mut ScreenBuffer, midi: Option<&str>, audio: Option<&str>) {
	buf.clear_all();
	controls.begin_frame();
	controls.place_dropdown(Dropdown::MidiIn, 0, 0, 20, midi, "no port");
	controls.place(ButtonId::Preset, 0, 1, "Preset", Look::Normal);
	controls.place_dropdown(Dropdown::AudioOut, 0, 2, 20, audio, "no device");
	controls.finish_frame(buf);
}

fn devices(midi: Option<&str>, audio: Option<&str>) -> (Controls, ScreenBuffer) {
	let mut controls = Controls::new();
	let mut buf = screen();
	device_frame(&mut controls, &mut buf, midi, audio);
	(controls, buf)
}

fn ports() -> Vec<String> {
	PORTS.iter().map(|port| (*port).to_owned()).collect()
}

/// The MIDI dropdown with `loopMIDI` in use, its list open and filled, and one more frame drawn —
/// the state the window is in after a click on the field.
fn open_midi_list() -> (Controls, ScreenBuffer) {
	let (mut controls, mut buf) = devices(Some("loopMIDI"), Some("Speakers"));
	assert!(controls.open(Dropdown::MidiIn));
	controls.set_items(Dropdown::MidiIn, ports());
	device_frame(&mut controls, &mut buf, Some("loopMIDI"), Some("Speakers"));
	// Drawing the layer is what tells its rows where they are.
	controls.layer(GRID).expect("the list is open");
	(controls, buf)
}

/// A left click, press and release, on a cell of the grid: what each of the two did.
fn click(controls: &mut Controls, col: i32, row: i32) -> (Touch, Touch) {
	let [press, release] = left_click(col, row);
	(controls.handle_mouse(&press, GRID), controls.handle_mouse(&release, GRID))
}

#[test]
fn a_dropdown_field_shows_the_device_in_use_or_says_in_red_that_there_is_none() {
	let (controls, buf) = devices(Some("KeyStep"), None);
	assert_eq!(text_at(&buf, 0, 0, 7), "KeyStep");
	assert_eq!(buf.cell(19, 0).ch, CLOSED, "the mark stands in the last cell of the field");
	assert_eq!((buf.cell(0, 0).fg, buf.cell(0, 0).bg), (TEXT, BACKGROUND));
	assert_eq!(text_at(&buf, 0, 2, 9), "no device");
	assert_eq!(buf.cell(19, 2).ch, CLOSED);
	assert_eq!(buf.cell(0, 2).fg, ERROR);
	assert_eq!(controls.open_dropdown(), None);
}

#[test]
fn the_ring_passes_through_the_dropdowns_in_the_order_of_the_screen() {
	let (mut controls, mut buf) = devices(Some("KeyStep"), Some("Speakers"));
	assert_eq!(
		controls.focused_dropdown(),
		Some(Dropdown::MidiIn),
		"the first control of the screen"
	);
	assert_eq!(controls.focused(), None);
	assert!(
		buf.cell(0, 0).flags.contains(CellFlags::UNDERLINE),
		"the field under the focus is underlined"
	);

	assert_eq!(controls.handle_key(&key(KeyCode::Tab)), Touch::Changed);
	assert_eq!(controls.focused(), Some(ButtonId::Preset));
	assert_eq!(controls.focused_dropdown(), None);
	assert_eq!(controls.handle_key(&key(KeyCode::Tab)), Touch::Changed);
	assert_eq!(controls.focused_dropdown(), Some(Dropdown::AudioOut));
	device_frame(&mut controls, &mut buf, Some("KeyStep"), Some("Speakers"));
	assert!(buf.cell(0, 2).flags.contains(CellFlags::UNDERLINE));
	assert!(!buf.cell(0, 0).flags.contains(CellFlags::UNDERLINE));

	assert_eq!(controls.handle_key(&key(KeyCode::Tab)), Touch::Changed);
	assert_eq!(controls.focused_dropdown(), Some(Dropdown::MidiIn), "round to the first");
	assert_eq!(controls.handle_key(&key(KeyCode::BackTab)), Touch::Changed);
	assert_eq!(controls.focused_dropdown(), Some(Dropdown::AudioOut));
}

#[test]
fn a_click_on_the_field_or_enter_under_the_focus_drops_the_list_down() {
	let (mut controls, mut buf) = devices(Some("loopMIDI"), Some("Speakers"));
	let (press, _) = click(&mut controls, 5, 0);
	assert_eq!(press, Touch::Opened(Dropdown::MidiIn), "the window is told to fill the list");
	assert_eq!(controls.open_dropdown(), Some(Dropdown::MidiIn));

	controls.set_items(Dropdown::MidiIn, ports());
	device_frame(&mut controls, &mut buf, Some("loopMIDI"), Some("Speakers"));
	assert_eq!(buf.cell(19, 0).ch, OPEN, "the mark of the field turns");
	let (area, list) = controls.layer(GRID).expect("the list is open");
	assert_eq!(
		area,
		Rect::new(0, 1, 20, 5),
		"under the field, as wide as the field, three rows in a frame"
	);
	let lines = lines_of(&list);
	assert!(lines[1].contains("KeyStep") && lines[2].contains("loopMIDI") && lines[3].contains("Roland"));
	assert_eq!(
		colours_of(&list, "loopMIDI"),
		(BACKGROUND, TEXT),
		"the list starts on the device in use"
	);
	assert_eq!(colours_of(&list, "KeyStep"), (TEXT, BACKGROUND));

	// Enter under the focus does the same for the other dropdown.
	let (mut controls, _) = devices(Some("loopMIDI"), Some("Speakers"));
	controls.handle_key(&key(KeyCode::Tab));
	controls.handle_key(&key(KeyCode::Tab));
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Opened(Dropdown::AudioOut));
	assert_eq!(controls.open_dropdown(), Some(Dropdown::AudioOut));
}

#[test]
fn the_key_of_a_dropdown_opens_its_list_and_takes_the_focus() {
	let (mut controls, _) = devices(Some("KeyStep"), Some("Speakers"));
	assert!(controls.open(Dropdown::AudioOut));
	assert_eq!(controls.open_dropdown(), Some(Dropdown::AudioOut));
	assert_eq!(controls.focused_dropdown(), Some(Dropdown::AudioOut));
	// Opening the other one closes this one: one list at a time.
	assert!(controls.open(Dropdown::MidiIn));
	assert_eq!(controls.open_dropdown(), Some(Dropdown::MidiIn));
	assert_eq!(controls.focused_dropdown(), Some(Dropdown::MidiIn));
}

#[test]
fn the_arrows_walk_an_open_list_and_enter_chooses_another_device() {
	let (mut controls, _) = open_midi_list();
	assert_eq!(controls.handle_key(&key(KeyCode::Down)), Touch::Changed);
	let (_, list) = controls.layer(GRID).expect("still open");
	assert_eq!(colours_of(&list, "Roland"), (BACKGROUND, TEXT));
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Chosen(Dropdown::MidiIn, 2));
	assert_eq!(controls.open_dropdown(), None, "choosing closes the list");

	let (mut controls, _) = open_midi_list();
	assert_eq!(controls.handle_key(&key(KeyCode::Home)), Touch::Changed);
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Chosen(Dropdown::MidiIn, 0));

	let (mut controls, _) = open_midi_list();
	controls.handle_key(&key(KeyCode::End));
	controls.handle_key(&key(KeyCode::Up));
	controls.handle_key(&key(KeyCode::Up));
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Chosen(Dropdown::MidiIn, 0));
}

#[test]
fn choosing_the_device_in_use_closes_the_list_and_asks_for_nothing() {
	let (mut controls, _) = open_midi_list();
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Changed);
	assert_eq!(controls.open_dropdown(), None);
}

#[test]
fn esc_and_a_click_outside_close_the_list_and_the_next_click_is_an_ordinary_one() {
	let (mut controls, mut buf) = open_midi_list();
	assert_eq!(controls.handle_key(&key(KeyCode::Esc)), Touch::Changed);
	assert_eq!(controls.open_dropdown(), None);
	device_frame(&mut controls, &mut buf, Some("loopMIDI"), Some("Speakers"));
	assert_eq!(buf.cell(19, 0).ch, CLOSED);

	// Opened with the mouse and closed with a click away from the list.
	click(&mut controls, 5, 0);
	controls.set_items(Dropdown::MidiIn, ports());
	device_frame(&mut controls, &mut buf, Some("loopMIDI"), Some("Speakers"));
	controls.layer(GRID).expect("open");
	// The press closes the list; the release that follows is for nobody.
	assert_eq!(click(&mut controls, 30, 12), (Touch::Changed, Touch::Missed));
	assert_eq!(controls.open_dropdown(), None);
	device_frame(&mut controls, &mut buf, Some("loopMIDI"), Some("Speakers"));

	// The field does not hold the pointer after its list has gone: a button is pressed as usual.
	assert_eq!(click(&mut controls, 1, 1), (Touch::Changed, Touch::Pressed(ButtonId::Preset)));
	assert_eq!(controls.open_dropdown(), None);
}

#[test]
fn a_click_on_a_row_of_the_list_chooses_it() {
	let (mut controls, _) = open_midi_list();
	let (area, list) = controls.layer(GRID).expect("open");
	let (col, row) = cell_of(area, &list, "KeyStep");
	let (press, _) = click(&mut controls, col, row);
	assert_eq!(press, Touch::Chosen(Dropdown::MidiIn, 0));
	assert_eq!(controls.open_dropdown(), None);
}

#[test]
fn the_wheel_walks_an_open_list() {
	let (mut controls, _) = open_midi_list();
	let (area, list) = controls.layer(GRID).expect("open");
	let (col, row) = cell_of(area, &list, "loopMIDI");
	let mut wheel = mouse(MouseKind::Wheel { lines: -1.0 }, col, row);
	wheel.button = None;
	assert_eq!(controls.handle_mouse(&wheel, GRID), Touch::Changed);
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Chosen(Dropdown::MidiIn, 2));
}

#[test]
fn while_a_list_is_open_the_screen_under_it_gets_nothing() {
	let (mut controls, _) = open_midi_list();
	for code in [KeyCode::Char('z'), KeyCode::Char(' '), KeyCode::F(5), KeyCode::Left] {
		assert_eq!(controls.handle_key(&key(code)), Touch::Missed, "{code:?}");
		assert_eq!(controls.open_dropdown(), Some(Dropdown::MidiIn), "{code:?} leaves the list open");
	}
	// Tab stays in the list: the ring of the screen is behind it.
	controls.handle_key(&key(KeyCode::Tab));
	assert_eq!(controls.focused_dropdown(), Some(Dropdown::MidiIn));
	assert_eq!(controls.open_dropdown(), Some(Dropdown::MidiIn));

	// A click on the button under the list closes the list and presses nothing.
	let (mut controls, _) = devices(Some("loopMIDI"), Some("Speakers"));
	assert!(controls.open(Dropdown::AudioOut));
	controls.set_items(Dropdown::AudioOut, vec!["Speakers".to_owned(), "HDMI".to_owned()]);
	controls.layer(GRID).expect("open");
	assert_eq!(click(&mut controls, 1, 1), (Touch::Changed, Touch::Missed));
	assert_eq!(controls.open_dropdown(), None);
}

#[test]
fn a_dropdown_that_left_the_screen_closes_its_list() {
	let (mut controls, mut buf) = open_midi_list();
	controls.begin_frame();
	controls.finish_frame(&mut buf);
	assert_eq!(controls.open_dropdown(), None);
	assert!(controls.layer(GRID).is_none());
}

#[test]
fn a_device_that_is_not_in_the_list_is_still_named_by_the_field() {
	// The list is filled when it is opened; until then the field has only the name to show.
	let (mut controls, mut buf) = devices(Some("Unlisted port"), Some("Speakers"));
	assert_eq!(text_at(&buf, 0, 0, 13), "Unlisted port");
	controls.set_items(Dropdown::MidiIn, ports());
	device_frame(&mut controls, &mut buf, Some("Unlisted port"), Some("Speakers"));
	assert_eq!(text_at(&buf, 0, 0, 13), "Unlisted port");
	// With the device in the list the field shows the row of the list.
	device_frame(&mut controls, &mut buf, Some("Roland"), Some("Speakers"));
	assert_eq!(text_at(&buf, 0, 0, 6), "Roland");
}

// --- the format dropdown -------------------------------------------------------------------------

const FORMATS: [&str; 3] = ["FLAC", "OGG", "MIDI"];

/// A frame with the format dropdown and the button that follows it on the screen. Row 0: the field,
/// columns 0..8. Row 1: `[Stop]` at 0..6.
fn format_frame(controls: &mut Controls, buf: &mut ScreenBuffer, is_locked: bool) {
	buf.clear_all();
	controls.begin_frame();
	controls.place_dropdown(Dropdown::Format, 0, 0, 8, Some("FLAC"), "");
	if is_locked {
		controls.lock_dropdown(Dropdown::Format);
	}
	controls.place(ButtonId::RecordStop, 0, 1, "Stop", Look::Normal);
	controls.finish_frame(buf);
}

fn formats() -> Vec<String> {
	FORMATS.iter().map(|format| (*format).to_owned()).collect()
}

#[test]
fn the_format_dropdown_opens_like_a_device_one_and_a_row_chooses_the_format() {
	let mut controls = Controls::new();
	let mut buf = screen();
	format_frame(&mut controls, &mut buf, false);
	assert_eq!(text_at(&buf, 0, 0, 8), "FLAC\0\0\0▾", "the format in use and the mark");
	assert_eq!((buf.cell(0, 0).fg, buf.cell(0, 0).bg), (TEXT, BACKGROUND));
	assert_eq!(
		controls.focused_dropdown(),
		Some(Dropdown::Format),
		"it stands in the ring before the buttons of the take"
	);

	let (press, _) = click(&mut controls, 1, 0);
	assert_eq!(press, Touch::Opened(Dropdown::Format));
	controls.set_items(Dropdown::Format, formats());
	format_frame(&mut controls, &mut buf, false);
	let (_, list) = controls.layer(GRID).expect("the list is open");
	let lines = lines_of(&list);
	assert!(lines[1].contains("FLAC") && lines[2].contains("OGG") && lines[3].contains("MIDI"));
	assert_eq!(
		colours_of(&list, "FLAC"),
		(BACKGROUND, TEXT),
		"the list starts on the format in use"
	);

	assert_eq!(controls.handle_key(&key(KeyCode::End)), Touch::Changed);
	assert_eq!(controls.handle_key(&key(KeyCode::Enter)), Touch::Chosen(Dropdown::Format, 2));
	assert_eq!(controls.open_dropdown(), None);
}

#[test]
fn a_locked_dropdown_shows_its_value_dark_and_neither_opens_nor_takes_the_focus() {
	let mut controls = Controls::new();
	let mut buf = screen();
	format_frame(&mut controls, &mut buf, false);
	assert!(controls.open(Dropdown::Format));
	controls.set_items(Dropdown::Format, formats());

	// A take started while the list was open: the list goes, the field stays and says what is recorded.
	format_frame(&mut controls, &mut buf, true);
	assert_eq!(controls.open_dropdown(), None, "a locked dropdown cannot keep its list open");
	assert!(controls.layer(GRID).is_none());
	assert_eq!(text_at(&buf, 0, 0, 4), "FLAC");
	assert_eq!((buf.cell(0, 0).fg, buf.cell(0, 0).bg), (DISABLED, BACKGROUND));
	assert!(!buf.cell(0, 0).flags.contains(CellFlags::UNDERLINE));
	assert_eq!(controls.focused_dropdown(), None);
	assert_eq!(controls.focused(), Some(ButtonId::RecordStop), "the focus went on to what reacts");

	assert_eq!(click(&mut controls, 1, 0), (Touch::Missed, Touch::Missed));
	assert!(!controls.open(Dropdown::Format), "its key does not open it either");
	controls.handle_key(&key(KeyCode::Tab));
	assert_eq!(controls.focused(), Some(ButtonId::RecordStop), "Tab does not stop at it");

	// The take ended: the field reacts again.
	format_frame(&mut controls, &mut buf, false);
	assert_eq!(buf.cell(0, 0).fg, TEXT);
	let (press, _) = click(&mut controls, 1, 0);
	assert_eq!(press, Touch::Opened(Dropdown::Format));
}
