//! Tests of the main screen buttons: placed in a `ScreenBuffer`, pressed through the same entry points
//! the window uses. No window.

use gterm::cell::CellFlags;
use gterm_widgets::MouseKind;
use winit::event::MouseButton;

use super::*;

fn screen() -> ScreenBuffer {
	ScreenBuffer::with_default_colors(40, 4, TEXT, BACKGROUND)
}

/// A frame with four buttons in a row: two that react, one unavailable, one more that reacts.
/// Columns: `[In]` 0..4, `[Flac]` 5..11, `[Midi]` 12..18, `[Stop]` 19..25.
/// The buffer is cleared first, as the screen does: a style is laid over what a cell already has.
fn frame(buttons: &mut Buttons, buf: &mut ScreenBuffer) {
	buf.clear_all();
	buttons.begin_frame();
	let mut x = 0;
	for (id, label, look) in [
		(ButtonId::MidiIn, "In", Look::Normal),
		(ButtonId::FormatFlac, "Flac", Look::Active),
		(ButtonId::FormatMidi, "Midi", Look::Disabled),
		(ButtonId::RecordStop, "Stop", Look::Normal),
	] {
		x += buttons.place(id, x, 1, label, look) + 1;
	}
	buttons.finish_frame(buf);
}

fn drawn() -> (Buttons, ScreenBuffer) {
	let mut buttons = Buttons::new();
	let mut buf = screen();
	frame(&mut buttons, &mut buf);
	(buttons, buf)
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
	}
}

fn text_at(buf: &ScreenBuffer, col: usize, row: usize, cells: usize) -> String {
	(col..col + cells).map(|col| buf.cell(col, row).ch).collect()
}

#[test]
fn a_button_takes_the_cells_of_its_label_in_brackets() {
	let mut buttons = Buttons::new();
	buttons.begin_frame();
	assert_eq!(buttons.place(ButtonId::MidiIn, 3, 2, "MIDI-вход", Look::Normal), 11);
	let mut buf = screen();
	buttons.finish_frame(&mut buf);
	assert_eq!(text_at(&buf, 3, 2, 11), "[MIDI-вход]");
	assert_eq!(buf.cell(2, 2).ch, char::MIN, "nothing is drawn around the button");
	assert_eq!(buf.cell(14, 2).ch, char::MIN);
}

#[test]
fn looks_are_drawn_as_the_spec_describes_them() {
	let (_, buf) = drawn();
	// Normal, and under the focus (the first button that reacts has it from the start): underlined.
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

	// The selected one that cannot be changed looks active.
	let mut buttons = Buttons::new();
	buttons.begin_frame();
	buttons.place(ButtonId::FormatFlac, 0, 0, "Flac", Look::ActiveLocked);
	let mut buf = screen();
	buttons.finish_frame(&mut buf);
	assert_eq!((buf.cell(0, 0).fg, buf.cell(0, 0).bg), (BACKGROUND, VALUE));
	assert_eq!(buttons.focused(), None, "it does not take the focus");
}

#[test]
fn tab_walks_the_buttons_that_react_in_a_ring_and_shift_tab_walks_back() {
	let (mut buttons, mut buf) = drawn();
	assert_eq!(buttons.focused(), Some(ButtonId::MidiIn));
	let mut walked = Vec::new();
	for _ in 0..4 {
		assert_eq!(buttons.handle_key(&key(KeyCode::Tab)), Touch::Changed);
		walked.push(buttons.focused().expect("a button has the focus"));
	}
	assert_eq!(
		walked,
		[ButtonId::FormatFlac, ButtonId::RecordStop, ButtonId::MidiIn, ButtonId::FormatFlac],
		"the unavailable button is skipped"
	);
	assert_eq!(buttons.handle_key(&key(KeyCode::BackTab)), Touch::Changed);
	assert_eq!(buttons.focused(), Some(ButtonId::MidiIn));
	assert_eq!(buttons.handle_key(&key(KeyCode::BackTab)), Touch::Changed);
	assert_eq!(buttons.focused(), Some(ButtonId::RecordStop), "back from the first is the last");

	// The next frame underlines the button the focus moved to.
	frame(&mut buttons, &mut buf);
	assert!(buf.cell(19, 1).flags.contains(CellFlags::UNDERLINE));
	assert!(!buf.cell(0, 1).flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn enter_presses_the_button_under_the_focus_and_other_keys_are_not_for_buttons() {
	let (mut buttons, _) = drawn();
	assert_eq!(buttons.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::MidiIn));
	buttons.handle_key(&key(KeyCode::Tab));
	assert_eq!(buttons.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::FormatFlac));

	for code in [KeyCode::Char(' '), KeyCode::Char('z'), KeyCode::Esc, KeyCode::Up, KeyCode::F(2)] {
		assert_eq!(buttons.handle_key(&key(code)), Touch::Missed, "{code:?}");
	}
	let mut release = key(KeyCode::Enter);
	release.kind = KeyEventKind::Release;
	assert_eq!(buttons.handle_key(&release), Touch::Missed, "letting go of Enter presses nothing");
	for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
		let chord = KeyEvent::new(KeyCode::Enter, modifiers);
		assert_eq!(buttons.handle_key(&chord), Touch::Missed, "{modifiers:?}+Enter");
		let chord = KeyEvent::new(KeyCode::Tab, modifiers);
		assert_eq!(buttons.handle_key(&chord), Touch::Missed, "{modifiers:?}+Tab");
	}
	assert_eq!(buttons.focused(), Some(ButtonId::FormatFlac), "none of that moved the focus");
}

#[test]
fn a_click_presses_on_release_takes_the_focus_and_shows_the_button_held() {
	let (mut buttons, mut buf) = drawn();
	// Any cell of the button will do: the last one of `[Stop]`.
	assert_eq!(buttons.handle_mouse(&mouse(MouseKind::Down, 24, 1)), Touch::Changed);
	assert_eq!(buttons.held(), Some(ButtonId::RecordStop));
	assert_eq!(buttons.focused(), Some(ButtonId::RecordStop), "a click moves the focus");
	frame(&mut buttons, &mut buf);
	assert_eq!(
		(buf.cell(19, 1).fg, buf.cell(19, 1).bg),
		(BACKGROUND, TEXT),
		"held down: inverted on light"
	);
	assert_eq!(
		buttons.handle_mouse(&mouse(MouseKind::Up, 24, 1)),
		Touch::Pressed(ButtonId::RecordStop)
	);
	assert_eq!(buttons.held(), None);
	frame(&mut buttons, &mut buf);
	assert_eq!((buf.cell(19, 1).fg, buf.cell(19, 1).bg), (TEXT, BACKGROUND));
}

#[test]
fn a_click_is_called_off_by_releasing_elsewhere_or_leaving_the_window() {
	let (mut buttons, _) = drawn();
	buttons.handle_mouse(&mouse(MouseKind::Down, 0, 1));
	assert_eq!(buttons.handle_mouse(&mouse(MouseKind::Move, 30, 3)), Touch::Changed);
	assert_eq!(
		buttons.handle_mouse(&mouse(MouseKind::Up, 30, 3)),
		Touch::Changed,
		"released away from the button"
	);
	assert_eq!(buttons.held(), None);

	buttons.handle_mouse(&mouse(MouseKind::Down, 0, 1));
	assert_eq!(buttons.handle_mouse(&mouse(MouseKind::Cancel, -1, -1)), Touch::Changed);
	assert_eq!(buttons.held(), None);
	assert_eq!(
		buttons.handle_mouse(&mouse(MouseKind::Up, 0, 1)),
		Touch::Missed,
		"the release after leaving presses nothing"
	);
}

#[test]
fn the_mouse_away_from_buttons_and_on_an_unavailable_one_does_nothing() {
	let (mut buttons, _) = drawn();
	for (col, row) in [(4, 1), (0, 0), (0, 2), (30, 1), (-1, -1)] {
		for kind in [MouseKind::Move, MouseKind::Down, MouseKind::Up] {
			assert_eq!(
				buttons.handle_mouse(&mouse(kind, col, row)),
				Touch::Missed,
				"{kind:?} at {col},{row}"
			);
		}
	}
	// `[Midi]` is unavailable.
	for kind in [MouseKind::Down, MouseKind::Up] {
		assert_eq!(buttons.handle_mouse(&mouse(kind, 13, 1)), Touch::Missed);
	}
	assert_eq!(buttons.focused(), Some(ButtonId::MidiIn), "the focus stays");
	// Moving over a button that reacts changes nothing either: there is no hover look.
	assert_eq!(buttons.handle_mouse(&mouse(MouseKind::Move, 6, 1)), Touch::Missed);
	// The right button is not a click.
	let mut right = mouse(MouseKind::Down, 0, 1);
	right.button = Some(MouseButton::Right);
	assert_eq!(buttons.handle_mouse(&right), Touch::Missed);
}

#[test]
fn before_the_first_frame_and_without_placed_buttons_nothing_reacts() {
	let mut buttons = Buttons::new();
	assert_eq!(buttons.handle_key(&key(KeyCode::Enter)), Touch::Missed);
	assert_eq!(buttons.handle_mouse(&mouse(MouseKind::Down, 0, 1)), Touch::Missed);

	// A frame that places nothing (the window is too small) switches every button off.
	let (mut buttons, mut buf) = drawn();
	buttons.begin_frame();
	buttons.finish_frame(&mut buf);
	assert_eq!(buttons.focused(), None);
	assert_eq!(buttons.handle_key(&key(KeyCode::Enter)), Touch::Missed);
	assert_eq!(buttons.handle_mouse(&mouse(MouseKind::Down, 0, 1)), Touch::Missed);
}

#[test]
fn the_focus_leaves_a_button_that_stopped_reacting_for_the_next_one_that_does() {
	let mut buttons = Buttons::new();
	let mut buf = screen();
	let mut take = |buttons: &mut Buttons, is_recording: bool| {
		let unless = |unavailable: bool| if unavailable { Look::Disabled } else { Look::Normal };
		buttons.begin_frame();
		buttons.place(ButtonId::MidiIn, 0, 0, "In", Look::Normal);
		buttons.place(ButtonId::RecordStart, 0, 1, "Start", unless(is_recording));
		buttons.place(ButtonId::RecordStop, 8, 1, "Stop", unless(!is_recording));
		buttons.finish_frame(&mut buf);
	};
	take(&mut buttons, false);
	buttons.handle_key(&key(KeyCode::Tab));
	assert_eq!(buttons.focused(), Some(ButtonId::RecordStart));
	assert_eq!(buttons.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::RecordStart));

	// The take runs: `[Start]` is unavailable, the focus is on `[Stop]`, Enter stops.
	take(&mut buttons, true);
	assert_eq!(buttons.focused(), Some(ButtonId::RecordStop));
	assert_eq!(buttons.handle_key(&key(KeyCode::Enter)), Touch::Pressed(ButtonId::RecordStop));

	// Stopped: from `[Stop]`, the last button, the ring goes round to the first.
	take(&mut buttons, false);
	assert_eq!(buttons.focused(), Some(ButtonId::MidiIn));
}
