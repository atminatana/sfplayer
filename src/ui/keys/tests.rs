//! Key table tests: defaults, chord parsing, config overrides, help generation.

use super::*;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
	KeyEvent::new(code, modifiers)
}

fn action_for(key: &KeyEvent) -> Option<Action> {
	Bindings::default().action_for(key)
}

#[test]
fn function_keys_and_ctrl_c_map_to_actions() {
	assert_eq!(action_for(&press(KeyCode::F(1), KeyModifiers::NONE)), Some(Action::Help));
	assert_eq!(action_for(&press(KeyCode::F(10), KeyModifiers::NONE)), Some(Action::Quit));
	assert_eq!(action_for(&press(KeyCode::F(2), KeyModifiers::NONE)), Some(Action::MidiDialog));
	assert_eq!(action_for(&press(KeyCode::F(3), KeyModifiers::NONE)), Some(Action::AudioDialog));
	assert_eq!(action_for(&press(KeyCode::F(4), KeyModifiers::NONE)), Some(Action::Sf2Dialog));
	assert_eq!(action_for(&press(KeyCode::F(5), KeyModifiers::NONE)), Some(Action::PresetDialog));
	assert_eq!(
		action_for(&press(KeyCode::F(6), KeyModifiers::NONE)),
		Some(Action::RecordFormatDialog)
	);
	assert_eq!(action_for(&press(KeyCode::F(7), KeyModifiers::NONE)), Some(Action::Record));
	assert_eq!(action_for(&press(KeyCode::F(8), KeyModifiers::NONE)), Some(Action::Rescan));
	assert_eq!(action_for(&press(KeyCode::F(9), KeyModifiers::NONE)), Some(Action::Panic));
	assert_eq!(action_for(&press(KeyCode::Char(' '), KeyModifiers::NONE)), Some(Action::Panic));
	assert_eq!(action_for(&press(KeyCode::Up, KeyModifiers::CONTROL)), Some(Action::OctaveUp));
	assert_eq!(action_for(&press(KeyCode::Left, KeyModifiers::CONTROL)), Some(Action::VelocityDown));
	assert_eq!(action_for(&press(KeyCode::Esc, KeyModifiers::NONE)), Some(Action::NotesOff));
	assert_eq!(
		action_for(&press(KeyCode::Char(']'), KeyModifiers::NONE)),
		Some(Action::TransposeUp)
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('['), KeyModifiers::NONE)),
		Some(Action::TransposeDown)
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('r'), KeyModifiers::ALT)),
		Some(Action::ToggleReverb)
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('c'), KeyModifiers::ALT)),
		Some(Action::ToggleChorus)
	);
	assert_eq!(action_for(&press(KeyCode::Char('c'), KeyModifiers::CONTROL)), Some(Action::Quit));
	assert_eq!(action_for(&press(KeyCode::Tab, KeyModifiers::NONE)), Some(Action::FocusNext));
	assert_eq!(
		action_for(&press(KeyCode::BackTab, KeyModifiers::SHIFT)),
		Some(Action::FocusPrevious)
	);
}

#[test]
fn plain_letters_are_never_commands() {
	for letter in "zxcvbnm,./sdghjl;".chars() {
		assert_eq!(action_for(&press(KeyCode::Char(letter), KeyModifiers::NONE)), None, "{letter}");
		assert_eq!(action_for(&press(KeyCode::Char(letter), KeyModifiers::SHIFT)), None, "{letter}");
	}
}

#[test]
fn no_default_binding_uses_a_bare_letter() {
	for binding in BINDINGS {
		if let KeyCode::Char(c) = binding.code
			&& c.is_alphabetic()
		{
			assert!(
				binding.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT),
				"{binding:?}"
			);
		}
	}
}

#[test]
fn mixer_keys_map_to_actions() {
	assert_eq!(action_for(&press(KeyCode::Char('+'), KeyModifiers::NONE)), Some(Action::MasterUp));
	assert_eq!(action_for(&press(KeyCode::Char('='), KeyModifiers::NONE)), Some(Action::MasterUp));
	assert_eq!(
		action_for(&press(KeyCode::Char('+'), KeyModifiers::SHIFT)),
		Some(Action::MasterUpFast)
	);
	assert_eq!(action_for(&press(KeyCode::Char('-'), KeyModifiers::NONE)), Some(Action::MasterDown));
	assert_eq!(
		action_for(&press(KeyCode::Char('_'), KeyModifiers::SHIFT)),
		Some(Action::MasterDownFast)
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('u'), KeyModifiers::CONTROL)),
		Some(Action::MasterMute)
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('0'), KeyModifiers::NONE)),
		Some(Action::SelectChannel(9))
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('6'), KeyModifiers::ALT)),
		Some(Action::SelectChannel(15))
	);
	assert_eq!(action_for(&press(KeyCode::Right, KeyModifiers::NONE)), Some(Action::ChannelUp));
	assert_eq!(
		action_for(&press(KeyCode::Char('x'), KeyModifiers::CONTROL)),
		Some(Action::ChannelMute)
	);
	assert_eq!(
		action_for(&press(KeyCode::Char('o'), KeyModifiers::CONTROL)),
		Some(Action::ChannelSolo)
	);
}

#[test]
fn help_lists_every_bound_action_with_its_chords() {
	let bindings = Bindings::default();
	let rows = bindings.help_rows("en");
	for binding in BINDINGS {
		if matches!(binding.action, Action::SelectChannel(_)) {
			continue;
		}
		let label = key_label(binding);
		assert!(
			rows.iter().any(|(keys, _)| keys.split(" / ").any(|chord| chord == label)),
			"{label} missing from help"
		);
	}
	assert!(
		rows.iter()
			.any(|(keys, description)| keys.contains("Alt+1") && description.contains("channel")),
		"grouped channel row"
	);
	let quit = rows.iter().find(|(keys, _)| keys.contains("F10")).expect("quit row");
	assert_eq!(quit.0, "F10 / Ctrl+C");
	assert!(
		!quit.1.is_empty() && !quit.1.starts_with("help."),
		"description must be translated: {}",
		quit.1
	);
	let distinct: std::collections::HashSet<&String> = rows.iter().map(|(_, description)| description).collect();
	assert_eq!(distinct.len(), rows.len(), "every action has its own description");
}

#[test]
fn key_labels_are_readable() {
	assert_eq!(key_label(&bind(Action::Quit, KeyCode::Char('c'), KeyModifiers::CONTROL)), "Ctrl+C");
	assert_eq!(
		key_label(&bind(Action::FocusPrevious, KeyCode::BackTab, KeyModifiers::SHIFT)),
		"Shift+Tab"
	);
	assert_eq!(key_label(&bind(Action::Help, KeyCode::F(1), KeyModifiers::NONE)), "F1");
	assert_eq!(key_label(&bind(Action::Help, KeyCode::Up, KeyModifiers::CONTROL)), "Ctrl+\u{2191}");
}

#[test]
fn chords_parse_in_any_case_and_label_round_trips() {
	assert_eq!(
		parse_chord("ctrl+alt+f5"),
		Some((KeyCode::F(5), KeyModifiers::CONTROL | KeyModifiers::ALT))
	);
	assert_eq!(parse_chord("Shift+Tab"), Some((KeyCode::BackTab, KeyModifiers::SHIFT)));
	assert_eq!(parse_chord("Space"), Some((KeyCode::Char(' '), KeyModifiers::NONE)));
	assert_eq!(parse_chord("]"), Some((KeyCode::Char(']'), KeyModifiers::NONE)));
	assert_eq!(parse_chord("+"), Some((KeyCode::Char('+'), KeyModifiers::NONE)));
	assert_eq!(parse_chord("Shift++"), Some((KeyCode::Char('+'), KeyModifiers::SHIFT)));
	assert_eq!(parse_chord("Ctrl+↑"), Some((KeyCode::Up, KeyModifiers::CONTROL)));
	assert_eq!(parse_chord("Ctrl+Q"), Some((KeyCode::Char('q'), KeyModifiers::CONTROL)));
	assert_eq!(parse_chord("PgDn"), Some((KeyCode::PageDown, KeyModifiers::NONE)));
	assert_eq!(parse_chord("Hyper+X"), None);
	assert_eq!(parse_chord("F99"), None);
	assert_eq!(parse_chord("ab"), None);
	for binding in BINDINGS {
		let (code, modifiers) = parse_chord(&key_label(binding)).unwrap_or_else(|| panic!("{}", key_label(binding)));
		assert_eq!(code, binding.code, "{}", key_label(binding));
		// BackTab is labelled "Shift+Tab" whether or not the terminal reported the Shift modifier.
		if binding.code != KeyCode::BackTab {
			assert_eq!(modifiers, binding.modifiers, "{}", key_label(binding));
		}
	}
}

#[test]
fn config_overrides_replace_default_chords() {
	let mut keys = BTreeMap::new();
	keys.insert("quit".to_owned(), "Ctrl+Q, F12".to_owned());
	keys.insert("select_channel_3".to_owned(), "Alt+F3".to_owned());
	let bindings = Bindings::from_config(&keys, &KeyboardConfig::default()).expect("valid overrides");
	assert_eq!(
		bindings.action_for(&press(KeyCode::Char('q'), KeyModifiers::CONTROL)),
		Some(Action::Quit)
	);
	assert_eq!(bindings.action_for(&press(KeyCode::F(12), KeyModifiers::NONE)), Some(Action::Quit));
	assert_eq!(
		bindings.action_for(&press(KeyCode::F(10), KeyModifiers::NONE)),
		None,
		"default chord dropped"
	);
	assert_eq!(bindings.action_for(&press(KeyCode::Char('c'), KeyModifiers::CONTROL)), None);
	assert_eq!(
		bindings.action_for(&press(KeyCode::F(3), KeyModifiers::ALT)),
		Some(Action::SelectChannel(2))
	);
	assert_eq!(bindings.action_for(&press(KeyCode::Char('3'), KeyModifiers::NONE)), None);
	let quit_row = bindings
		.help_rows("en")
		.into_iter()
		.find(|(_, description)| description == "Quit")
		.expect("quit row");
	assert_eq!(quit_row.0, "Ctrl+Q / F12", "help follows the overrides");
}

#[test]
fn keyboard_alternatives_are_added_on_top_of_the_defaults() {
	let keyboard = KeyboardConfig {
		octave_up: Some("F11".to_owned()),
		velocity_down: Some("Alt+Down".to_owned()),
		..KeyboardConfig::default()
	};
	let bindings = Bindings::from_config(&BTreeMap::new(), &keyboard).expect("valid");
	assert_eq!(
		bindings.action_for(&press(KeyCode::F(11), KeyModifiers::NONE)),
		Some(Action::OctaveUp)
	);
	assert_eq!(
		bindings.action_for(&press(KeyCode::Up, KeyModifiers::CONTROL)),
		Some(Action::OctaveUp),
		"default kept"
	);
	assert_eq!(
		bindings.action_for(&press(KeyCode::Down, KeyModifiers::ALT)),
		Some(Action::VelocityDown)
	);
}

#[test]
fn bad_overrides_are_rejected_with_the_offending_key() {
	let mut keys = BTreeMap::new();
	keys.insert("fly".to_owned(), "F12".to_owned());
	assert_eq!(
		Bindings::from_config(&keys, &KeyboardConfig::default()),
		Err(BindingError::UnknownCommand { key: "fly".into() })
	);
	keys.clear();
	keys.insert("quit".to_owned(), "Hyper+Q".to_owned());
	assert_eq!(
		Bindings::from_config(&keys, &KeyboardConfig::default()),
		Err(BindingError::BadChord {
			key: "quit".into(),
			chord: "Hyper+Q".into()
		})
	);
	keys.clear();
	keys.insert("panic".to_owned(), "p".to_owned());
	assert_eq!(
		Bindings::from_config(&keys, &KeyboardConfig::default()),
		Err(BindingError::BareLetter {
			key: "panic".into(),
			chord: "p".into()
		})
	);
	keys.clear();
	keys.insert("panic".to_owned(), "F1".to_owned());
	assert_eq!(
		Bindings::from_config(&keys, &KeyboardConfig::default()),
		Err(BindingError::Conflict {
			key: "panic".into(),
			chord: "F1".into(),
			other: "help".into()
		})
	);
	assert_eq!(command_name(Action::SelectChannel(15)), "select_channel_16");
	assert_eq!(action_by_name("select_channel_17"), None);
}
