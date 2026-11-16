//! Tests of the preset window: keys and the mouse go through the routing entry points the window
//! uses, the picture is read back from the modal layer's buffer. No window, no bank file.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::{KeyMsg, MemoryClipboard, MouseMsg, Rect};
use winit::keyboard::{ModifiersState, SmolStr};

use super::*;
use crate::synth::PresetInfo;
use crate::ui::testkit::{SCREEN, cell_of, colours_of, find, is_shown, is_underlined, left_click, left_double_click, lines_of, wheel};

fn preset(bank: u16, program: u8, name: &str) -> PresetInfo {
	PresetInfo {
		bank,
		program,
		name: name.to_owned(),
	}
}

fn bank() -> Vec<PresetInfo> {
	vec![
		preset(0, 0, "Grand Piano"),
		preset(0, 1, "Bright Piano"),
		preset(0, 4, "Electric Piano"),
		preset(0, 19, "Church Organ"),
		preset(0, 40, "Violin"),
		preset(128, 0, "Standard Kit"),
	]
}

fn dialog(current: Option<(u16, u8)>) -> PresetDialog {
	PresetDialog::new(&PresetRequest { presets: bank(), current })
}

/// A bank with more presets than the list has rows.
fn long_dialog() -> PresetDialog {
	PresetDialog::new(&PresetRequest {
		presets: (0..40).map(|program| preset(0, program, &format!("Voice {program:02}"))).collect(),
		current: None,
	})
}

/// One key press, routed and settled the way the window does it.
fn press_with(dialog: &mut PresetDialog, key: &Key, text: Option<&str>, modifiers: ModifiersState) -> PresetOutcome {
	let mut clipboard = MemoryClipboard::new();
	let message = KeyMsg { key, text, modifiers };
	let routed = dialog.ui("ru", &mut clipboard).route_key(&message);
	dialog.settle(routed, Some(key))
}

fn press(dialog: &mut PresetDialog, named: NamedKey) -> PresetOutcome {
	press_with(dialog, &Key::Named(named), None, ModifiersState::empty())
}

fn type_text(dialog: &mut PresetDialog, text: &str) {
	for character in text.chars() {
		let typed = character.to_string();
		let key = Key::Character(SmolStr::new(&typed));
		let outcome = press_with(dialog, &key, Some(&typed), ModifiersState::empty());
		assert_eq!(outcome, PresetOutcome::Pending, "typing {character:?} keeps the window open");
	}
}

/// The window as drawn: where it sits on the screen and its cells.
fn drawn(dialog: &mut PresetDialog, locale: &str) -> (Rect, ScreenBuffer) {
	let mut clipboard = MemoryClipboard::new();
	dialog
		.ui(locale, &mut clipboard)
		.modal_buffer(SCREEN)
		.expect("the window is a modal layer")
}

/// Mouse messages routed and settled one after another; the answer of whichever gave one.
fn send(dialog: &mut PresetDialog, messages: &[MouseMsg]) -> PresetOutcome {
	let mut outcome = PresetOutcome::Pending;
	for message in messages {
		let mut clipboard = MemoryClipboard::new();
		let routed = dialog.ui("ru", &mut clipboard).route_mouse(message, SCREEN);
		let step = dialog.settle(routed, None);
		if step != PresetOutcome::Pending {
			outcome = step;
		}
	}
	outcome
}

/// The last cell inside the frame in the row of the window that shows `text`: where the scrollbar of
/// the list stands.
fn last_cell_of_the_row(buf: &ScreenBuffer, text: &str) -> char {
	let (row, _) = find(buf, text);
	match buf.cell(usize::from(WIDTH) - 2, row).ch {
		char::MIN => ' ',
		ch => ch,
	}
}

/// The screen cell of the first character of `text` in the window as it is drawn now.
fn place_of(dialog: &mut PresetDialog, text: &str) -> (i32, i32) {
	let (area, buf) = drawn(dialog, "ru");
	cell_of(area, &buf, text)
}

fn click(dialog: &mut PresetDialog, text: &str) -> PresetOutcome {
	let (col, row) = place_of(dialog, text);
	send(dialog, &left_click(col, row))
}

fn double_click(dialog: &mut PresetDialog, text: &str) -> PresetOutcome {
	let (col, row) = place_of(dialog, text);
	send(dialog, &left_double_click(col, row))
}

#[test]
fn the_window_shows_the_filter_the_presets_and_the_buttons() {
	let mut dialog = dialog(Some((0, 4)));
	let (area, buf) = drawn(&mut dialog, "ru");
	assert_eq!((area.width, area.height), (WIDTH, HEIGHT));
	assert_eq!(area.x, (SCREEN.width - WIDTH) / 2, "the window is centred");
	let lines = lines_of(&buf);
	assert!(lines[0].starts_with("┌ Пресет (все каналы, кроме 10) ─"), "{}", lines[0]);
	assert!(lines[1].starts_with("│Фильтр:"), "{}", lines[1]);
	assert!(is_shown(&buf, "  000:000 Grand Piano"));
	assert!(is_shown(&buf, "● 000:004 Electric Piano"), "the preset in use is marked");
	assert!(is_shown(&buf, "  128:000 Standard Kit"));
	assert!(is_shown(&buf, "двойной клик — применить"));
	assert!(is_shown(&buf, "[OK] [Отмена]"));
	assert_eq!(
		colours_of(&buf, "● 000:004"),
		(BACKGROUND, TEXT),
		"the list starts on the preset in use"
	);
	assert_eq!(colours_of(&buf, "  000:000"), (TEXT, BACKGROUND));
	assert_eq!(
		last_cell_of_the_row(&buf, "000:000 Grand Piano"),
		' ',
		"a list that fits has no scrollbar"
	);

	let (_, english) = drawn(&mut dialog, "en");
	assert!(lines_of(&english)[0].starts_with("┌ Preset (all channels except 10) ─"));
	assert!(is_shown(&english, "[OK] [Cancel]"));
}

#[test]
fn the_list_starts_on_the_preset_in_use_or_on_the_first_one() {
	assert_eq!(dialog(Some((0, 19))).highlighted(), Some((0, 19)));
	assert_eq!(dialog(None).highlighted(), Some((0, 0)), "the bank plays its first preset");
	assert_eq!(dialog(Some((5, 5))).highlighted(), Some((0, 0)), "a preset the bank does not have");
}

#[test]
fn typing_narrows_the_list_whatever_the_case() {
	let mut dialog = dialog(Some((0, 40)));
	type_text(&mut dialog, "PIANO");
	assert_eq!(dialog.filter(), "PIANO");
	assert_eq!(
		dialog.shown(),
		["000:000 Grand Piano", "000:001 Bright Piano", "000:004 Electric Piano"]
	);
	assert_eq!(dialog.highlighted(), Some((0, 0)), "the highlight goes to the first match");
	// The numbers of the line are searched as well as the name.
	let mut by_number = self::dialog(None);
	type_text(&mut by_number, "128");
	assert_eq!(by_number.shown(), ["128:000 Standard Kit"]);
	// Deleting a character widens the list again.
	press(&mut by_number, NamedKey::Backspace);
	press(&mut by_number, NamedKey::Backspace);
	press(&mut by_number, NamedKey::Backspace);
	assert_eq!(by_number.shown().len(), 6);
}

#[test]
fn with_no_match_the_window_says_so_and_nothing_can_be_chosen() {
	let mut dialog = dialog(None);
	type_text(&mut dialog, "zzz");
	assert!(dialog.shown().is_empty());
	assert_eq!(dialog.highlighted(), None);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(is_shown(&buf, "нет совпадений"));
	assert_eq!(press(&mut dialog, NamedKey::Enter), PresetOutcome::Pending);
	assert_eq!(click(&mut dialog, "[OK]"), PresetOutcome::Pending);
	assert_eq!(press(&mut dialog, NamedKey::ArrowDown), PresetOutcome::Pending);

	let mut empty = PresetDialog::new(&PresetRequest {
		presets: Vec::new(),
		current: None,
	});
	let (_, buf) = drawn(&mut empty, "ru");
	assert!(is_shown(&buf, "выбирать нечего"));
	assert_eq!(press(&mut empty, NamedKey::Enter), PresetOutcome::Pending);
}

#[test]
fn the_arrows_walk_the_list_from_the_field_and_enter_chooses() {
	let mut dialog = dialog(None);
	assert_eq!(press(&mut dialog, NamedKey::ArrowDown), PresetOutcome::Pending);
	assert_eq!(dialog.highlighted(), Some((0, 1)));
	press(&mut dialog, NamedKey::ArrowDown);
	press(&mut dialog, NamedKey::ArrowUp);
	assert_eq!(dialog.highlighted(), Some((0, 1)));
	press(&mut dialog, NamedKey::PageDown);
	assert_eq!(dialog.highlighted(), Some((128, 0)), "a page down stops at the last preset");
	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(dialog.highlighted(), Some((128, 0)));
	press(&mut dialog, NamedKey::PageUp);
	assert_eq!(dialog.highlighted(), Some((0, 0)));
	press(&mut dialog, NamedKey::ArrowUp);
	assert_eq!(dialog.highlighted(), Some((0, 0)));

	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(press(&mut dialog, NamedKey::Enter), PresetOutcome::Chosen { bank: 0, program: 1 });
}

#[test]
fn enter_and_ok_choose_the_highlighted_preset() {
	assert_eq!(
		press(&mut dialog(Some((0, 19))), NamedKey::Enter),
		PresetOutcome::Chosen { bank: 0, program: 19 }
	);
	assert_eq!(
		click(&mut dialog(Some((0, 19))), "[OK]"),
		PresetOutcome::Chosen { bank: 0, program: 19 }
	);
	// After a filter the answer is the preset, not its place in the narrowed list.
	let mut filtered = dialog(None);
	type_text(&mut filtered, "organ");
	assert_eq!(
		press(&mut filtered, NamedKey::Enter),
		PresetOutcome::Chosen { bank: 0, program: 19 }
	);
}

#[test]
fn a_click_highlights_a_row_and_a_double_click_chooses_it() {
	let mut dialog = dialog(None);
	assert_eq!(click(&mut dialog, "000:040 Violin"), PresetOutcome::Pending);
	assert_eq!(dialog.highlighted(), Some((0, 40)), "one click only moves the highlight");
	assert_eq!(
		double_click(&mut dialog, "000:019 Church Organ"),
		PresetOutcome::Chosen { bank: 0, program: 19 }
	);
	// A double click on the frame or on the filter chooses nothing.
	let mut dialog = self::dialog(None);
	assert_eq!(double_click(&mut dialog, "Фильтр:"), PresetOutcome::Pending);
}

#[test]
fn esc_cancel_and_a_click_outside_close_the_window_without_choosing() {
	assert_eq!(press(&mut dialog(None), NamedKey::Escape), PresetOutcome::Cancelled);
	assert_eq!(click(&mut dialog(None), "[Отмена]"), PresetOutcome::Cancelled);
	assert_eq!(send(&mut dialog(None), &left_click(0, 0)), PresetOutcome::Cancelled);
}

#[test]
fn tab_goes_from_the_field_to_the_list_and_on_to_the_buttons() {
	let mut dialog = dialog(None);
	assert_eq!(press(&mut dialog, NamedKey::Tab), PresetOutcome::Pending);
	// In the list the arrows and End are the list's own keys.
	press(&mut dialog, NamedKey::ArrowDown);
	assert_eq!(dialog.highlighted(), Some((0, 1)));
	press(&mut dialog, NamedKey::End);
	assert_eq!(dialog.highlighted(), Some((128, 0)));
	assert_eq!(
		press(&mut dialog, NamedKey::Enter),
		PresetOutcome::Chosen { bank: 128, program: 0 },
		"Enter in the list chooses"
	);

	let mut dialog = self::dialog(None);
	press(&mut dialog, NamedKey::Tab);
	press(&mut dialog, NamedKey::Tab);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(is_underlined(&buf, "[OK]"));
	press(&mut dialog, NamedKey::Tab);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(is_underlined(&buf, "[Отмена]") && !is_underlined(&buf, "[OK]"));
	assert_eq!(press(&mut dialog, NamedKey::Enter), PresetOutcome::Cancelled);
}

#[test]
fn the_wheel_scrolls_a_long_list_and_leaves_the_highlight_where_it_is() {
	let mut dialog = long_dialog();
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(
		is_shown(&buf, "Voice 00") && !is_shown(&buf, "Voice 39"),
		"the list is longer than the window"
	);
	assert_ne!(last_cell_of_the_row(&buf, "Voice 03"), ' ', "a long list has a scrollbar");
	let (col, row) = place_of(&mut dialog, "000:003 Voice 03");
	assert_eq!(send(&mut dialog, &[wheel(col, row, -3.0)]), PresetOutcome::Pending);
	let (_, buf) = drawn(&mut dialog, "ru");
	assert!(!is_shown(&buf, "Voice 00"), "the list moved down");
	assert!(is_shown(&buf, "Voice 05"));
	assert_eq!(dialog.highlighted(), Some((0, 0)), "the wheel does not choose");
}

#[test]
fn lines_are_matched_by_a_part_of_them_whatever_the_case() {
	let lines = ["000:000 Grand Piano", "000:019 Church Organ", "128:000 Standard Kit"];
	assert_eq!(matching(lines.into_iter(), ""), [0, 1, 2]);
	assert_eq!(matching(lines.into_iter(), "an"), [0, 1, 2]);
	assert_eq!(matching(lines.into_iter(), "ORGAN"), [1]);
	assert_eq!(matching(lines.into_iter(), "000:0"), [0, 1]);
	assert!(matching(lines.into_iter(), "harp").is_empty());
}
