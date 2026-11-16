//! Tests of the confirmation window: keys and the mouse go through the routing entry points the
//! window uses, the picture is read back from the modal layer's buffer. No window.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::{KeyMsg, Rect};
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

use super::*;
use crate::ui::testkit::{SCREEN, cell_of, colours_of, is_shown, is_underlined, left_click, lines_of};

/// The running take as the program window names it for the question about deleting it.
const TAKE: &str = "00:42 · 4.6 МБ";

fn press_with(dialog: &mut ConfirmDialog, key: &Key, text: Option<&str>, modifiers: ModifiersState) -> ConfirmOutcome {
	let message = KeyMsg { key, text, modifiers };
	let routed = dialog.ui("ru", TAKE).route_key(&message);
	dialog.settle(routed)
}

fn press(dialog: &mut ConfirmDialog, named: NamedKey) -> ConfirmOutcome {
	press_with(dialog, &Key::Named(named), None, ModifiersState::empty())
}

/// The window as drawn: where it sits on the screen and its cells.
fn drawn(dialog: &mut ConfirmDialog, locale: &str) -> (Rect, ScreenBuffer) {
	dialog.ui(locale, TAKE).modal_buffer(SCREEN).expect("the window is a modal layer")
}

/// A left click on a screen cell; the answer of the press or of the release, whichever gave one.
fn click_at(dialog: &mut ConfirmDialog, col: i32, row: i32) -> ConfirmOutcome {
	let mut outcome = ConfirmOutcome::Pending;
	for message in left_click(col, row) {
		let routed = dialog.ui("ru", TAKE).route_mouse(&message, SCREEN);
		let step = dialog.settle(routed);
		if step != ConfirmOutcome::Pending {
			outcome = step;
		}
	}
	outcome
}

fn click(dialog: &mut ConfirmDialog, text: &str) -> ConfirmOutcome {
	let (area, buf) = drawn(dialog, "ru");
	let (col, row) = cell_of(area, &buf, text);
	click_at(dialog, col, row)
}

#[test]
fn the_window_shows_the_question_in_a_red_frame_with_the_focus_on_the_refusal() {
	let mut dialog = ConfirmDialog::new(Question::Quit);
	let (area, buf) = drawn(&mut dialog, "ru");
	assert_eq!((area.width, area.height), (WIDTH, HEIGHT));
	assert_eq!(area.x, (SCREEN.width - WIDTH) / 2, "the window is centred");
	let lines = lines_of(&buf);
	assert!(lines[0].starts_with("┌ Идёт запись ─"), "{}", lines[0]);
	assert!(is_shown(&buf, "Остановить запись и выйти?"));
	assert!(is_shown(&buf, "[Выйти]   [Нет]"));
	assert_eq!(colours_of(&buf, "┌").0, ERROR);
	assert!(is_underlined(&buf, "[Нет]"), "the refusal holds the focus");
	assert!(!is_underlined(&buf, "[Выйти]"));

	let (_, english) = drawn(&mut dialog, "en");
	assert!(lines_of(&english)[0].starts_with("┌ Recording in progress ─"));
	assert!(is_shown(&english, "Stop the recording and quit?"));
	assert!(is_shown(&english, "[Quit]   [No]"));
}

#[test]
fn the_question_about_a_take_names_its_length_and_size() {
	let mut dialog = ConfirmDialog::new(Question::DeleteTake);
	let (area, buf) = drawn(&mut dialog, "ru");
	assert_eq!((area.width, area.height), (WIDTH, HEIGHT));
	assert!(lines_of(&buf)[0].starts_with("┌ Отмена записи ─"), "{}", lines_of(&buf)[0]);
	assert!(is_shown(&buf, "Удалить запись 00:42 · 4.6 МБ?"));
	assert!(is_shown(&buf, "[Удалить]   [Нет]"));
	assert_eq!(colours_of(&buf, "┌").0, ERROR);
	assert!(is_underlined(&buf, "[Нет]") && !is_underlined(&buf, "[Удалить]"));

	let (_, english) = drawn(&mut dialog, "en");
	assert!(lines_of(&english)[0].starts_with("┌ Cancel the take ─"));
	assert!(is_shown(&english, "Delete the take 00:42 · 4.6 МБ?"));
	assert!(is_shown(&english, "[Delete]   [No]"));

	// The answers are those of any question.
	assert_eq!(press(&mut dialog, NamedKey::Enter), ConfirmOutcome::Refused);
	assert_eq!(
		click(&mut ConfirmDialog::new(Question::DeleteTake), "[Удалить]"),
		ConfirmOutcome::Confirmed
	);
}

#[test]
fn enter_or_space_right_after_opening_refuses() {
	assert_eq!(
		press(&mut ConfirmDialog::new(Question::Quit), NamedKey::Enter),
		ConfirmOutcome::Refused
	);
	assert_eq!(
		press(&mut ConfirmDialog::new(Question::Quit), NamedKey::Space),
		ConfirmOutcome::Refused
	);
}

#[test]
fn tab_moves_the_focus_to_the_action_and_enter_confirms() {
	let mut dialog = ConfirmDialog::new(Question::Quit);
	assert_eq!(press(&mut dialog, NamedKey::Tab), ConfirmOutcome::Pending);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(is_underlined(&buf, "[Выйти]") && !is_underlined(&buf, "[Нет]"));
	assert_eq!(press(&mut dialog, NamedKey::Enter), ConfirmOutcome::Confirmed);

	// The ring has two buttons: Shift+Tab from the refusal lands on the action as well.
	let mut dialog = ConfirmDialog::new(Question::Quit);
	let back = press_with(&mut dialog, &Key::Named(NamedKey::Tab), None, ModifiersState::SHIFT);
	assert_eq!(back, ConfirmOutcome::Pending);
	assert_eq!(press(&mut dialog, NamedKey::Enter), ConfirmOutcome::Confirmed);

	// And Tab twice comes back to the refusal.
	let mut dialog = ConfirmDialog::new(Question::Quit);
	press(&mut dialog, NamedKey::Tab);
	press(&mut dialog, NamedKey::Tab);
	assert_eq!(press(&mut dialog, NamedKey::Enter), ConfirmOutcome::Refused);
}

#[test]
fn esc_and_a_click_outside_the_window_refuse() {
	assert_eq!(
		press(&mut ConfirmDialog::new(Question::Quit), NamedKey::Escape),
		ConfirmOutcome::Refused
	);
	assert_eq!(click_at(&mut ConfirmDialog::new(Question::Quit), 0, 0), ConfirmOutcome::Refused);
}

#[test]
fn a_click_presses_the_button_under_it() {
	assert_eq!(click(&mut ConfirmDialog::new(Question::Quit), "[Выйти]"), ConfirmOutcome::Confirmed);
	assert_eq!(click(&mut ConfirmDialog::new(Question::Quit), "[Нет]"), ConfirmOutcome::Refused);
	// A click on the question itself is inside the window and presses nothing.
	assert_eq!(
		click(&mut ConfirmDialog::new(Question::Quit), "Остановить запись"),
		ConfirmOutcome::Pending
	);
}

#[test]
fn keys_that_are_not_for_the_buttons_mean_nothing() {
	let mut dialog = ConfirmDialog::new(Question::Quit);
	let letter = Key::Character(SmolStr::new("z"));
	assert_eq!(
		press_with(&mut dialog, &letter, Some("z"), ModifiersState::empty()),
		ConfirmOutcome::Pending
	);
	for named in [NamedKey::ArrowLeft, NamedKey::ArrowRight, NamedKey::F9, NamedKey::Home] {
		assert_eq!(press(&mut dialog, named), ConfirmOutcome::Pending, "{named:?}");
	}
	// The focus is where it was: the refusal.
	assert_eq!(press(&mut dialog, NamedKey::Enter), ConfirmOutcome::Refused);
}
