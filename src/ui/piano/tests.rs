//! Tests of the on-screen piano: the picture in a `ScreenBuffer`, which key a cell belongs to, and the
//! notes the mouse plays. No window.

use gterm::cell::Color;
use winit::keyboard::ModifiersState;

use super::*;
use crate::ui::paint::{BACKGROUND, LABEL, TEXT};

/// Where the tests put the piano on their grid.
const ORIGIN: (u16, u16) = (10, 3);
/// C4, the base key of the default text keyboard.
const BASE: u8 = 60;
const CHANNEL: u8 = 0;
const VELOCITY: u8 = 100;

/// The characters of the default text keyboard.
const WHITE: [char; 10] = ['z', 'x', 'c', 'v', 'b', 'n', 'm', ',', '.', '/'];
const BLACK: [char; 7] = ['s', 'd', 'g', 'h', 'j', 'l', ';'];
/// The four rows of a piano with those characters.
const ROWS: [&str; 4] = [
	"│  │s│ │d│  │  │g│ │h│ │j│  │  │l│ │;│  │",
	"│  └┬┘ └┬┘  │  └┬┘ └┬┘ └┬┘  │  └┬┘ └┬┘  │",
	"│ z │ x │ c │ v │ b │ n │ m │ , │ . │ / │",
	"└───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘",
];

fn screen() -> ScreenBuffer {
	ScreenBuffer::with_default_colors(60, 8, TEXT, BACKGROUND)
}

fn drawn_with(base_key: u8, labels: KeyLabels<'_>, sounding: u128) -> ScreenBuffer {
	let mut buf = screen();
	draw(&mut buf, ORIGIN.0, ORIGIN.1, base_key, labels, sounding);
	buf
}

fn drawn(base_key: u8, sounding: u128) -> ScreenBuffer {
	drawn_with(
		base_key,
		KeyLabels {
			white: &WHITE,
			black: &BLACK,
		},
		sounding,
	)
}

/// A row of the piano as text — a cell nothing was drawn into is a space — and the colour of each of
/// its cells.
fn row(buf: &ScreenBuffer, row: u16) -> (String, Vec<Color>) {
	let cells = (0..WIDTH).map(|column| buf.cell(usize::from(ORIGIN.0 + column), usize::from(ORIGIN.1 + row)));
	let text = cells
		.clone()
		.map(|cell| match cell.ch {
			char::MIN => ' ',
			ch => ch,
		})
		.collect();
	(text, cells.map(|cell| cell.fg).collect())
}

/// The columns of a piano row drawn the way a lit key is: dark on amber.
fn lit_columns(buf: &ScreenBuffer, piano_row: u16) -> Vec<usize> {
	(0..usize::from(WIDTH))
		.filter(|column| {
			let cell = buf.cell(usize::from(ORIGIN.0) + column, usize::from(ORIGIN.1 + piano_row));
			(cell.fg, cell.bg) == (BACKGROUND, VALUE)
		})
		.collect()
}

fn keys(sounding: &[u8]) -> u128 {
	sounding.iter().fold(0, |map, key| map | (1u128 << key))
}

/// A mouse event at a cell of the piano (column and row counted from its top-left cell).
fn at(kind: MouseKind, column: i32, row: i32) -> MouseMsg {
	MouseMsg {
		kind,
		button: matches!(kind, MouseKind::Down | MouseKind::Up).then_some(MouseButton::Left),
		col: i32::from(ORIGIN.0) + column,
		row: i32::from(ORIGIN.1) + row,
		modifiers: ModifiersState::empty(),
		click_count: u8::from(matches!(kind, MouseKind::Down | MouseKind::Up)),
	}
}

fn mouse(piano: &mut Piano, kind: MouseKind, column: i32, row: i32) -> Option<Vec<MidiEvent>> {
	piano.handle_mouse(&at(kind, column, row), Some(ORIGIN), BASE, VELOCITY)
}

fn on(key: u8) -> MidiEvent {
	MidiEvent::NoteOn {
		channel: CHANNEL,
		key,
		velocity: VELOCITY,
	}
}

fn off(key: u8) -> MidiEvent {
	MidiEvent::NoteOff { channel: CHANNEL, key }
}

#[test]
fn a_silent_piano_is_the_four_rows_of_the_mockup_with_the_character_of_each_key() {
	let buf = drawn(BASE, 0);
	for (piano_row, expected) in (0..HEIGHT).zip(ROWS) {
		let (text, colours) = row(&buf, piano_row);
		assert_eq!(text, expected, "row {piano_row}");
		assert_eq!(text.chars().count(), usize::from(WIDTH));
		// The lines are dark; the characters stand out of them.
		for (ch, colour) in text.chars().zip(colours) {
			let expected = match ch {
				' ' => continue,
				ch if ch.is_ascii_graphic() => LABEL,
				_ => HINT,
			};
			assert_eq!(colour, expected, "{ch:?} in row {piano_row}");
		}
		assert!(lit_columns(&buf, piano_row).is_empty(), "nothing sounds");
	}
	// Nothing is drawn around it.
	assert_eq!(buf.cell(usize::from(ORIGIN.0) - 1, usize::from(ORIGIN.1)).ch, char::MIN);
	assert_eq!(buf.cell(usize::from(ORIGIN.0 + WIDTH), usize::from(ORIGIN.1)).ch, char::MIN);
	assert_eq!(buf.cell(usize::from(ORIGIN.0), usize::from(ORIGIN.1 + HEIGHT)).ch, char::MIN);
}

#[test]
fn the_keys_carry_the_characters_of_the_layout_and_a_key_without_one_stays_blank() {
	// A layout of the config: other characters, and fewer of them than the piano has keys.
	let labels = KeyLabels {
		white: &['q', 'w', 'e'],
		black: &['2'],
	};
	let buf = drawn_with(BASE, labels, 0);
	let (top, _) = row(&buf, 0);
	let (bottom, _) = row(&buf, 2);
	assert_eq!(top, "│  │2│ │ │  │  │ │ │ │ │ │  │  │ │ │ │  │");
	assert_eq!(bottom, "│ q │ w │ e │   │   │   │   │   │   │   │");
}

#[test]
fn a_sounding_key_is_lit_at_its_character() {
	// C4 (the first white key), F#4 (a black one) and E5 (the last white key).
	let buf = drawn(BASE, keys(&[60, 66, 76]));
	assert_eq!(lit_columns(&buf, 0), [16], "the character of F#, over the border between F and G");
	assert!(lit_columns(&buf, 1).is_empty(), "the foot of a black key is never lit");
	assert_eq!(
		lit_columns(&buf, 2),
		[1, 2, 3, 37, 38, 39],
		"C is lit between its borders, and so is the last E"
	);
	// A lit key keeps its shape and its character: only the colours change.
	for (piano_row, expected) in (0..HEIGHT).zip(ROWS) {
		assert_eq!(row(&buf, piano_row).0, expected, "row {piano_row}");
	}

	// Every key of the range has its own cells: all seventeen lit.
	let all: Vec<u8> = (60..=76).collect();
	let buf = drawn(BASE, keys(&all));
	assert_eq!(lit_columns(&buf, 0).len(), 7);
	assert_eq!(lit_columns(&buf, 2).len(), 10 * 3);
}

#[test]
fn the_piano_shows_the_range_from_its_base_key_and_nothing_outside_it() {
	// Notes below the base C and above its E a tenth higher are not on the piano.
	let buf = drawn(BASE, keys(&[59, 77, 0, 127]));
	assert!((0..HEIGHT).all(|piano_row| lit_columns(&buf, piano_row).is_empty()));
	// An octave lower, C4 is the eighth white key.
	let buf = drawn(48, keys(&[60]));
	assert_eq!(lit_columns(&buf, 2), [29, 30, 31]);
	// At the top of the keyboard the range runs past key 127: those keys simply never light.
	let buf = drawn(120, keys(&[120, 127]));
	assert_eq!(lit_columns(&buf, 2), [1, 2, 3, 17, 18, 19], "C and G of the last octave");
}

#[test]
fn every_cell_of_the_piano_belongs_to_the_key_drawn_there() {
	let upper: Vec<u8> = (0..WIDTH).map(|column| key_at(column, 0).expect("inside")).collect();
	let foot: Vec<u8> = (0..WIDTH).map(|column| key_at(column, 1).expect("inside")).collect();
	let bottom: Vec<u8> = (0..WIDTH).map(|column| key_at(column, 2).expect("inside")).collect();
	let line: Vec<u8> = (0..WIDTH).map(|column| key_at(column, 3).expect("inside")).collect();
	assert_eq!(
		upper,
		[
			0, 0, 0, 1, 1, 1, 2, 3, 3, 3, 4, 4, // C C# D D# E
			5, 5, 5, 6, 6, 6, 7, 8, 8, 8, 9, 10, 10, 10, 11, 11, // F F# G G# A A# B
			12, 12, 12, 13, 13, 13, 14, 15, 15, 15, 16, 16, 16, // C C# D D# E and the right border
		]
	);
	assert_eq!(foot, upper, "a black key is two rows high");
	assert_eq!(
		bottom,
		[
			0, 0, 0, 0, 2, 2, 2, 2, 4, 4, 4, 4, 5, 5, 5, 5, 7, 7, 7, 7, 9, 9, 9, 9, 11, 11, 11, 11, 12, 12, 12, 12, 14, 14, 14, 14, 16, 16,
			16, 16, 16
		],
		"the row of the characters is white keys only"
	);
	assert_eq!(line, bottom, "and so is the line that closes them");
	assert_eq!(key_at(WIDTH, 0), None);
	assert_eq!(key_at(0, HEIGHT), None);
	// The cells that light up for a key are cells that play it.
	for offset in 0..=16u8 {
		let buf = drawn(BASE, keys(&[BASE + offset]));
		for piano_row in 0..HEIGHT {
			for column in lit_columns(&buf, piano_row) {
				assert_eq!(key_at(column as u16, piano_row), Some(offset), "cell {column},{piano_row}");
			}
		}
	}
}

#[test]
fn a_press_plays_the_key_under_the_pointer_and_the_release_stops_it() {
	let mut piano = Piano::new(CHANNEL);
	assert!(!piano.is_dragging());
	// The first white key, by its bottom cell; then a black key, by its top cell and by its foot.
	assert_eq!(mouse(&mut piano, MouseKind::Down, 2, 2), Some(vec![on(60)]));
	assert!(piano.is_dragging());
	assert_eq!(mouse(&mut piano, MouseKind::Up, 2, 2), Some(vec![off(60)]));
	assert!(!piano.is_dragging());
	assert_eq!(mouse(&mut piano, MouseKind::Down, 4, 0), Some(vec![on(61)]));
	assert_eq!(mouse(&mut piano, MouseKind::Up, 4, 0), Some(vec![off(61)]));
	assert_eq!(mouse(&mut piano, MouseKind::Down, 3, 1), Some(vec![on(61)]));
	assert_eq!(mouse(&mut piano, MouseKind::Up, 3, 1), Some(vec![off(61)]));
	// The piano follows the base octave and uses the velocity it is given.
	let down = at(MouseKind::Down, 38, 2);
	assert_eq!(
		piano.handle_mouse(&down, Some(ORIGIN), 36, 64),
		Some(vec![MidiEvent::NoteOn {
			channel: CHANNEL,
			key: 36 + 16,
			velocity: 64
		}])
	);
}

#[test]
fn dragging_over_the_keys_plays_each_in_turn() {
	let mut piano = Piano::new(CHANNEL);
	assert_eq!(mouse(&mut piano, MouseKind::Down, 1, 2), Some(vec![on(60)]));
	assert_eq!(
		mouse(&mut piano, MouseKind::Move, 3, 2),
		Some(Vec::new()),
		"still on the same key: nothing to play"
	);
	assert_eq!(mouse(&mut piano, MouseKind::Move, 5, 2), Some(vec![off(60), on(62)]));
	assert_eq!(
		mouse(&mut piano, MouseKind::Move, 8, 0),
		Some(vec![off(62), on(63)]),
		"up onto the black key"
	);
	assert_eq!(mouse(&mut piano, MouseKind::Up, 8, 0), Some(vec![off(63)]));
	assert_eq!(
		mouse(&mut piano, MouseKind::Move, 10, 2),
		None,
		"the button is up: moving plays nothing"
	);
}

#[test]
fn leaving_the_piano_stops_the_note_and_coming_back_starts_the_key_entered() {
	let mut piano = Piano::new(CHANNEL);
	mouse(&mut piano, MouseKind::Down, 1, 2);
	assert_eq!(mouse(&mut piano, MouseKind::Move, 1, 4), Some(vec![off(60)]), "below the piano");
	assert!(piano.is_dragging(), "the button is still down");
	assert_eq!(mouse(&mut piano, MouseKind::Move, -5, 7), Some(Vec::new()), "still outside");
	assert_eq!(mouse(&mut piano, MouseKind::Move, 10, 2), Some(vec![on(64)]));
	// Released outside: the note stopped when the pointer left, nothing more to send.
	mouse(&mut piano, MouseKind::Move, 41, 0);
	assert_eq!(mouse(&mut piano, MouseKind::Up, 41, 0), Some(Vec::new()));
	assert!(!piano.is_dragging());
}

#[test]
fn the_note_ends_when_the_pointer_leaves_the_window_or_the_window_loses_the_focus() {
	let mut piano = Piano::new(CHANNEL);
	mouse(&mut piano, MouseKind::Down, 1, 1);
	let left_the_window = MouseMsg {
		kind: MouseKind::Cancel,
		button: None,
		col: -1,
		row: -1,
		modifiers: ModifiersState::empty(),
		click_count: 0,
	};
	assert_eq!(
		piano.handle_mouse(&left_the_window, Some(ORIGIN), BASE, VELOCITY),
		Some(vec![off(60)])
	);
	assert!(!piano.is_dragging());

	mouse(&mut piano, MouseKind::Down, 5, 2);
	assert_eq!(piano.release(), vec![off(62)]);
	assert_eq!(piano.release(), Vec::new(), "nothing is held any more");
	assert_eq!(
		mouse(&mut piano, MouseKind::Up, 5, 2),
		None,
		"the release that follows is not the piano's"
	);
}

#[test]
fn the_note_that_ends_is_the_one_that_started_whatever_changed_meanwhile() {
	let mut piano = Piano::new(CHANNEL);
	mouse(&mut piano, MouseKind::Down, 1, 1);
	// The base octave went up while the button was down: the same cell is another key now.
	let moved = at(MouseKind::Move, 2, 1);
	assert_eq!(
		piano.handle_mouse(&moved, Some(ORIGIN), BASE + 12, VELOCITY),
		Some(vec![off(60), on(72)])
	);
	// The window shrank below the minimum and the piano is gone from the screen.
	let up = at(MouseKind::Up, 2, 1);
	assert_eq!(piano.handle_mouse(&up, None, BASE + 12, VELOCITY), Some(vec![off(72)]));
}

#[test]
fn what_is_not_a_left_press_on_a_key_is_not_for_the_piano() {
	let mut piano = Piano::new(CHANNEL);
	for (column, row) in [(-1, 0), (41, 0), (0, -1), (0, 4), (100, 100)] {
		assert_eq!(mouse(&mut piano, MouseKind::Down, column, row), None, "outside at {column},{row}");
	}
	assert_eq!(
		mouse(&mut piano, MouseKind::Move, 1, 1),
		None,
		"moving over the keys without a press"
	);
	assert_eq!(mouse(&mut piano, MouseKind::Up, 1, 1), None);
	assert_eq!(mouse(&mut piano, MouseKind::Wheel { lines: 1.0 }, 1, 1), None);
	let mut right = at(MouseKind::Down, 1, 1);
	right.button = Some(MouseButton::Right);
	assert_eq!(piano.handle_mouse(&right, Some(ORIGIN), BASE, VELOCITY), None);
	let press = at(MouseKind::Down, 1, 1);
	assert_eq!(
		piano.handle_mouse(&press, None, BASE, VELOCITY),
		None,
		"the piano is not on the screen"
	);
	assert!(!piano.is_dragging());

	// While the left button holds a note, the other buttons and the wheel are swallowed and change nothing.
	mouse(&mut piano, MouseKind::Down, 1, 1);
	assert_eq!(piano.handle_mouse(&right, Some(ORIGIN), BASE, VELOCITY), Some(Vec::new()));
	let mut right_up = at(MouseKind::Up, 1, 1);
	right_up.button = Some(MouseButton::Right);
	assert_eq!(piano.handle_mouse(&right_up, Some(ORIGIN), BASE, VELOCITY), Some(Vec::new()));
	assert_eq!(mouse(&mut piano, MouseKind::Wheel { lines: -1.0 }, 1, 1), Some(Vec::new()));
	assert_eq!(mouse(&mut piano, MouseKind::Up, 1, 1), Some(vec![off(60)]));
}

#[test]
fn keys_past_the_top_of_the_midi_range_do_not_play() {
	let mut piano = Piano::new(CHANNEL);
	// With the base at 120 the last white key would be 136.
	let press = at(MouseKind::Down, 38, 2);
	assert_eq!(piano.handle_mouse(&press, Some(ORIGIN), 120, VELOCITY), None);
	let press = at(MouseKind::Down, 18, 2);
	assert_eq!(
		piano.handle_mouse(&press, Some(ORIGIN), 120, VELOCITY),
		Some(vec![on(127)]),
		"G of that octave is the last MIDI key"
	);
}
