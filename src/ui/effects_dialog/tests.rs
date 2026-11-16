//! Tests of the effects window: keys and the mouse go through the routing entry points the window
//! uses, the picture is read back from the modal layer's buffer. No window, no synthesizer.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::{KeyMsg, MouseKind, MouseMsg, Rect};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::*;
use crate::ui::testkit::{SCREEN, cell_of, is_shown, is_underlined, left_click, lines_of, wheel};

fn settings() -> EffectsSettings {
	EffectsSettings {
		reverb: true,
		chorus: false,
		reverb_level: 40,
		chorus_level: 20,
	}
}

fn dialog() -> EffectsDialog {
	EffectsDialog::new(settings())
}

fn press_with(dialog: &mut EffectsDialog, named: NamedKey, modifiers: ModifiersState) -> EffectsOutcome {
	let key = Key::Named(named);
	let message = KeyMsg {
		key: &key,
		text: None,
		modifiers,
	};
	let routed = dialog.ui("ru").route_key(&message);
	dialog.settle(routed)
}

fn press(dialog: &mut EffectsDialog, named: NamedKey) -> EffectsOutcome {
	press_with(dialog, named, ModifiersState::empty())
}

/// The window as drawn: where it sits on the screen and its cells.
fn drawn(dialog: &mut EffectsDialog, locale: &str) -> (Rect, ScreenBuffer) {
	dialog.ui(locale).modal_buffer(SCREEN).expect("the window is a modal layer")
}

/// Mouse messages routed and settled one after another; what the last one that did something did.
fn send(dialog: &mut EffectsDialog, messages: &[MouseMsg]) -> EffectsOutcome {
	let mut outcome = EffectsOutcome::Pending;
	for message in messages {
		let routed = dialog.ui("ru").route_mouse(message, SCREEN);
		let step = dialog.settle(routed);
		if step != EffectsOutcome::Pending {
			outcome = step;
		}
	}
	outcome
}

fn click(dialog: &mut EffectsDialog, text: &str) -> EffectsOutcome {
	let (area, buf) = drawn(dialog, "ru");
	let (col, row) = cell_of(area, &buf, text);
	send(dialog, &left_click(col, row))
}

/// The screen cells of the track of a slider: its first and last column and its row. The reverb is the
/// first row inside the frame, the chorus the second.
fn track(dialog: &mut EffectsDialog, effect_row: u16) -> (i32, i32, i32) {
	let (area, _) = drawn(dialog, "ru");
	let first = i32::from(area.x + 1 + NAME_CELLS);
	let last = i32::from(area.x + WIDTH - 2 - VALUE_CELLS);
	(first, last, i32::from(area.y + 1 + effect_row))
}

fn mouse(kind: MouseKind, col: i32, row: i32) -> MouseMsg {
	let [press, _] = left_click(col, row);
	MouseMsg {
		kind,
		button: matches!(kind, MouseKind::Down | MouseKind::Up).then_some(winit::event::MouseButton::Left),
		click_count: u8::from(matches!(kind, MouseKind::Down | MouseKind::Up)),
		..press
	}
}

#[test]
fn the_window_shows_a_switch_a_slider_and_a_level_for_each_effect() {
	let mut dialog = dialog();
	let (area, buf) = drawn(&mut dialog, "ru");
	assert_eq!((area.width, area.height), (WIDTH, HEIGHT));
	assert_eq!(area.x, (SCREEN.width - WIDTH) / 2, "the window is centred");
	let lines = lines_of(&buf);
	assert!(lines[0].starts_with("┌ Эффекты ─"), "{}", lines[0]);
	assert!(lines[1].starts_with("│[x] Реверб"), "{}", lines[1]);
	assert!(lines[2].starts_with("│[ ] Хорус"), "{}", lines[2]);
	assert!(lines[1].trim_end_matches('│').ends_with(" 40"), "{}", lines[1]);
	assert!(lines[2].trim_end_matches('│').ends_with(" 20"), "{}", lines[2]);
	assert!(lines[1].contains('█') && lines[2].contains('█'), "each row has a slider");
	assert!(is_shown(&buf, "[OK] [Отмена]"));
	assert!(is_underlined(&buf, "[x] Реверб"), "the first control holds the focus");

	let (_, english) = drawn(&mut dialog, "en");
	assert!(lines_of(&english)[1].starts_with("│[x] Reverb"));
	assert!(is_shown(&english, "[OK] [Cancel]"));
}

#[test]
fn a_switch_is_turned_by_space_enter_or_a_click_and_the_change_is_reported_at_once() {
	let mut dialog = dialog();
	let off = EffectsSettings {
		reverb: false,
		..settings()
	};
	assert_eq!(press(&mut dialog, NamedKey::Space), EffectsOutcome::Changed(off));
	assert_eq!(
		press(&mut dialog, NamedKey::Enter),
		EffectsOutcome::Changed(settings()),
		"Enter turns the switch too"
	);
	assert_eq!(
		click(&mut dialog, "[ ] Хорус"),
		EffectsOutcome::Changed(EffectsSettings {
			chorus: true,
			..settings()
		})
	);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(is_shown(&buf, "[x] Хорус"));
}

#[test]
fn the_arrows_move_the_level_under_the_focus_by_five_and_home_and_end_go_to_the_limits() {
	let mut dialog = dialog();
	press(&mut dialog, NamedKey::Tab);
	let with_level = |reverb_level| EffectsSettings {
		reverb_level,
		..settings()
	};
	assert_eq!(press(&mut dialog, NamedKey::ArrowRight), EffectsOutcome::Changed(with_level(45)));
	assert_eq!(press(&mut dialog, NamedKey::ArrowLeft), EffectsOutcome::Changed(with_level(40)));
	assert_eq!(press(&mut dialog, NamedKey::End), EffectsOutcome::Changed(with_level(127)));
	assert_eq!(
		press(&mut dialog, NamedKey::ArrowRight),
		EffectsOutcome::Pending,
		"at the limit nothing changes"
	);
	assert_eq!(press(&mut dialog, NamedKey::Home), EffectsOutcome::Changed(with_level(0)));
	assert_eq!(press(&mut dialog, NamedKey::ArrowLeft), EffectsOutcome::Pending);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(
		lines_of(&buf)[1].trim_end_matches('│').ends_with("  0"),
		"the read-out follows the slider"
	);

	// The slider of the chorus is two more stops along the ring.
	press(&mut dialog, NamedKey::Tab);
	press(&mut dialog, NamedKey::Tab);
	assert_eq!(
		press(&mut dialog, NamedKey::ArrowRight),
		EffectsOutcome::Changed(EffectsSettings {
			reverb_level: 0,
			chorus_level: 25,
			..settings()
		})
	);
}

#[test]
fn a_click_on_the_track_sets_the_level_and_a_drag_leads_it() {
	let mut dialog = dialog();
	let (first, last, row) = track(&mut dialog, 0);
	let level_after = |outcome: EffectsOutcome| match outcome {
		EffectsOutcome::Changed(settings) => settings.reverb_level,
		other => panic!("the level did not change: {other:?}"),
	};
	assert_eq!(
		level_after(send(&mut dialog, &left_click(last, row))),
		127,
		"the right end is the maximum"
	);
	assert_eq!(level_after(send(&mut dialog, &left_click(first, row))), 0, "the left end is zero");
	let middle = level_after(send(&mut dialog, &left_click((first + last) / 2, row)));
	assert!((60..=67).contains(&middle), "the middle of the track is about half: {middle}");

	// Pressed at the left end and dragged to the right one — off the track, even — and released.
	let drag = [
		mouse(MouseKind::Down, first, row),
		mouse(MouseKind::Move, last, row + 3),
		mouse(MouseKind::Up, last, row + 3),
	];
	assert_eq!(level_after(send(&mut dialog, &drag)), 127);
	// Once the button is up the pointer leads nothing.
	assert_eq!(send(&mut dialog, &[mouse(MouseKind::Move, first, row)]), EffectsOutcome::Pending);
	assert_eq!(dialog.settings().reverb_level, 127);
}

#[test]
fn the_wheel_moves_the_level_under_the_pointer_by_five() {
	let mut dialog = dialog();
	let (first, _, row) = track(&mut dialog, 1);
	let with_chorus = |chorus_level| EffectsSettings {
		chorus_level,
		..settings()
	};
	assert_eq!(
		send(&mut dialog, &[wheel(first + 3, row, 1.0)]),
		EffectsOutcome::Changed(with_chorus(25))
	);
	assert_eq!(
		send(&mut dialog, &[wheel(first + 3, row, -2.0)]),
		EffectsOutcome::Changed(with_chorus(15))
	);
}

#[test]
fn ok_keeps_the_changes() {
	let mut dialog = dialog();
	press(&mut dialog, NamedKey::Space);
	let off = EffectsSettings {
		reverb: false,
		..settings()
	};
	assert_eq!(click(&mut dialog, "[OK]"), EffectsOutcome::Accepted(off));

	// With the keyboard: four stops along the ring to `[OK]`, Enter presses it.
	let mut dialog = self::dialog();
	for _ in 0..4 {
		press(&mut dialog, NamedKey::Tab);
	}
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(is_underlined(&buf, "[OK]"));
	assert_eq!(press(&mut dialog, NamedKey::Enter), EffectsOutcome::Accepted(settings()));
}

#[test]
fn leaving_without_ok_goes_back_to_what_the_window_opened_with() {
	// Nothing was changed: there is nothing to go back to.
	assert_eq!(press(&mut dialog(), NamedKey::Escape), EffectsOutcome::Cancelled(None));
	assert_eq!(click(&mut dialog(), "[Отмена]"), EffectsOutcome::Cancelled(None));
	assert_eq!(send(&mut dialog(), &left_click(0, 0)), EffectsOutcome::Cancelled(None));

	let mut changed = dialog();
	press(&mut changed, NamedKey::Space);
	assert_eq!(press(&mut changed, NamedKey::Escape), EffectsOutcome::Cancelled(Some(settings())));
	let mut changed = dialog();
	press(&mut changed, NamedKey::Space);
	assert_eq!(click(&mut changed, "[Отмена]"), EffectsOutcome::Cancelled(Some(settings())));
	let mut changed = dialog();
	press(&mut changed, NamedKey::Space);
	assert_eq!(send(&mut changed, &left_click(0, 0)), EffectsOutcome::Cancelled(Some(settings())));

	// Changed and changed back is not a change.
	let mut same = dialog();
	press(&mut same, NamedKey::Space);
	press(&mut same, NamedKey::Space);
	assert_eq!(press(&mut same, NamedKey::Escape), EffectsOutcome::Cancelled(None));

	// The program window leaves the same way when the quit command closes the window.
	assert_eq!(dialog().cancel(), EffectsOutcome::Cancelled(None));
	let mut changed = dialog();
	press(&mut changed, NamedKey::Space);
	assert_eq!(changed.cancel(), EffectsOutcome::Cancelled(Some(settings())));
}

#[test]
fn a_level_above_the_range_is_shown_at_its_limit() {
	let mut dialog = EffectsDialog::new(EffectsSettings {
		reverb_level: 200,
		..settings()
	});
	assert_eq!(dialog.settings().reverb_level, MAX_LEVEL);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(lines_of(&buf)[1].trim_end_matches('│').ends_with("127"));
}
