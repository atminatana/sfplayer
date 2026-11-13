//! Tests of what the session sends to the audio thread and of the rules it applies, on rings and plain
//! values: none of them needs an audio device.

use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Split};

use super::*;

/// The global preset reaches channels 1 and 16 and skips channel 10 (spec v1.1 §7, criterion 3).
#[test]
fn global_preset_selects_bank_and_program_on_every_channel_but_percussion() {
	let (mut commands, mut received) = HeapRb::<Command>::new(64).split();
	push_global_preset(&mut commands, 8, 5);
	let mut programs = Vec::new();
	let mut bank_selects = Vec::new();
	while let Some(command) = received.try_pop() {
		match command {
			Command::Midi(MidiEvent::ProgramChange { channel, program }) => programs.push((channel, program)),
			Command::Midi(MidiEvent::ControlChange {
				channel,
				controller: controller::BANK_SELECT,
				value,
			}) => bank_selects.push((channel, value)),
			_ => panic!("unexpected command: only Bank Select and Program Change belong here"),
		}
	}
	let expected_channels: Vec<u8> = (0..16).filter(|channel| *channel != PERCUSSION_CHANNEL).collect();
	assert_eq!(programs.iter().map(|(channel, _)| *channel).collect::<Vec<_>>(), expected_channels);
	assert_eq!(
		bank_selects.iter().map(|(channel, _)| *channel).collect::<Vec<_>>(),
		expected_channels
	);
	assert!(programs.iter().all(|(_, program)| *program == 5));
	assert!(bank_selects.iter().all(|(_, bank)| *bank == 8));
	assert!(!programs.iter().any(|(channel, _)| *channel == 9), "channel 10 keeps its drum kit");
}

/// RMS of what an engine renders for one note held a tenth of a second.
fn loudness_of_a_note(engine: &mut dyn SynthEngine) -> f32 {
	engine.note_on(0, 60, 100);
	let mut left = vec![0.0f32; 4_800];
	let mut right = vec![0.0f32; 4_800];
	engine.render(&mut left, &mut right);
	(left.iter().map(|sample| sample * sample).sum::<f32>() / left.len() as f32).sqrt()
}

#[test]
fn the_builtin_synth_needs_no_bank_and_a_sound_font_is_silent_without_one() {
	let timbre = Arc::new(Timbre::new(Wave::Saw, crate::config::BuiltinConfig::default().adsr()));
	let mut engine = build_engine(EngineKind::Builtin, None, &timbre, 48_000, true)
		.expect("nothing to fail")
		.expect("an engine without a bank");
	assert_eq!(engine.max_voices(), 64);
	assert!(loudness_of_a_note(engine.as_mut()) > 0.01);
	// The engine shares the timbre it was given: a change reaches its next note.
	assert_eq!(Arc::strong_count(&timbre), 2);

	assert!(
		build_engine(EngineKind::Sf2, None, &timbre, 48_000, true)
			.expect("no error")
			.is_none()
	);

	let bank = SoundFontBank::load(std::path::Path::new(crate::synth::test_support::TEST_BANK_PATH)).expect("test bank");
	let mut engine = build_engine(EngineKind::Sf2, Some(&bank), &timbre, 48_000, true)
		.expect("the bank fits the rate")
		.expect("an engine for the bank");
	assert!(loudness_of_a_note(engine.as_mut()) > 0.01);
	// With a bank at hand the built-in synthesizer is still the built-in synthesizer.
	let engine = build_engine(EngineKind::Builtin, Some(&bank), &timbre, 48_000, false).expect("no error");
	assert_eq!(engine.map(|engine| engine.max_voices()), Some(64));
	assert_eq!(Arc::strong_count(&timbre), 2, "the dropped engines let go of the timbre");
	// A rate the SoundFont engine cannot work at is an error, not a silent processor.
	assert!(build_engine(EngineKind::Sf2, Some(&bank), &timbre, 1_000, true).is_err());
}

#[test]
fn only_a_sound_font_needs_a_new_engine_when_its_effects_appear_or_go() {
	assert!(needs_new_engine(EngineKind::Sf2, false, true), "the first effect goes on");
	assert!(needs_new_engine(EngineKind::Sf2, true, false), "the last effect goes off");
	assert!(
		!needs_new_engine(EngineKind::Sf2, true, true),
		"a level changed, or one of two effects"
	);
	assert!(!needs_new_engine(EngineKind::Sf2, false, false));
	for (had, has) in [(false, true), (true, false), (true, true), (false, false)] {
		assert!(
			!needs_new_engine(EngineKind::Builtin, had, has),
			"the built-in synthesizer keeps its engine"
		);
	}
}

/// Everything a ring holds, as (channel, what) pairs of the messages that matter to these tests.
fn sent(received: &mut ringbuf::HeapCons<Command>) -> Vec<MidiEvent> {
	let mut events = Vec::new();
	while let Some(command) = received.try_pop() {
		match command {
			Command::Midi(event) => events.push(event),
			_ => panic!("only service messages are expected here"),
		}
	}
	events
}

#[test]
fn after_a_midi_file_every_channel_is_put_back_in_order_and_the_preset_and_sends_return() {
	let (mut commands, mut received) = HeapRb::<Command>::new(256).split();
	let synth = SynthConfig::default();
	push_after_playback(&mut commands, Some((0, 5)), &synth);
	let events = sent(&mut received);
	let control = |channel: u8, controller: u8| {
		events.iter().find_map(|event| match event {
			MidiEvent::ControlChange {
				channel: found,
				controller: number,
				value,
			} if (*found, *number) == (channel, controller) => Some(*value),
			_ => None,
		})
	};
	for channel in 0..16 {
		assert_eq!(control(channel, controller::RESET_ALL_CONTROLLERS), Some(0), "channel {channel}");
		assert_eq!(control(channel, controller::VOLUME), Some(100));
		assert_eq!(control(channel, controller::PAN), Some(64));
		// The sends of the default config: reverb at 40, chorus off.
		assert_eq!(control(channel, controller::REVERB_SEND), Some(40));
		assert_eq!(control(channel, controller::CHORUS_SEND), Some(0));
	}
	// The F5 preset again on every channel but percussion, after the controllers were reset.
	let programs: Vec<(u8, u8)> = events
		.iter()
		.filter_map(|event| match event {
			MidiEvent::ProgramChange { channel, program } => Some((*channel, *program)),
			_ => None,
		})
		.collect();
	assert_eq!(programs.len(), 15);
	assert!(
		programs
			.iter()
			.all(|(channel, program)| *channel != PERCUSSION_CHANNEL && *program == 5)
	);
	let position = |wanted: &dyn Fn(&MidiEvent) -> bool| events.iter().position(wanted).expect("sent");
	let last_reset = events
		.iter()
		.rposition(|event| {
			matches!(
				event,
				MidiEvent::ControlChange {
					controller: controller::RESET_ALL_CONTROLLERS,
					..
				}
			)
		})
		.expect("sent");
	assert!(last_reset < position(&|event| matches!(event, MidiEvent::ProgramChange { .. })));
	assert_eq!(events.len(), 16 * 3 + 15 * 2 + 16 * 2, "and it all fits the command ring");

	// Without a chosen preset none is sent; the built-in synthesizer has no presets at all.
	push_after_playback(&mut commands, None, &synth);
	assert!(
		!sent(&mut received)
			.iter()
			.any(|event| matches!(event, MidiEvent::ProgramChange { .. }))
	);
	let builtin = SynthConfig {
		engine: EngineKind::Builtin,
		..SynthConfig::default()
	};
	push_after_playback(&mut commands, Some((0, 5)), &builtin);
	let events = sent(&mut received);
	assert!(!events.iter().any(|event| matches!(event, MidiEvent::ProgramChange { .. })));
	assert_eq!(events.len(), 16 * 3 + 16 * 2);
}

#[test]
fn a_midi_file_that_is_not_there_is_told_apart_from_one_that_cannot_be_played() {
	let directory = std::env::temp_dir().join(format!("sf2_player-control-test-{}", std::process::id()));
	std::fs::create_dir_all(&directory).expect("scratch directory");
	let missing = directory.join("gone.mid");
	let error = smf::load(&missing).expect_err("no such file");
	assert_eq!(midi_file_problem(&missing, &error), MidiFile::Missing(missing.clone()));

	let not_midi = directory.join("text.mid");
	std::fs::write(&not_midi, b"not a MIDI file at all").expect("write");
	let error = smf::load(&not_midi).expect_err("not a MIDI file");
	assert_eq!(
		midi_file_problem(&not_midi, &error),
		MidiFile::Broken {
			path: not_midi.clone(),
			reason: "not a MIDI file: it does not start with the MThd header".to_owned(),
		}
	);
	// A folder in the place of a file is there, and is not a file that can be played.
	let error = smf::load(&directory).expect_err("a folder");
	assert!(matches!(midi_file_problem(&directory, &error), MidiFile::Broken { .. }));
	std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn notes_played_in_the_window_travel_as_play_commands_in_order() {
	let (mut commands, mut received) = HeapRb::<Command>::new(2).split();
	let played = [
		MidiEvent::NoteOn {
			channel: 0,
			key: 60,
			velocity: 100,
		},
		MidiEvent::NoteOff { channel: 0, key: 60 },
		MidiEvent::NoteOff { channel: 0, key: 62 },
	];
	push_played(&mut commands, &played);
	let mut arrived = Vec::new();
	while let Some(command) = received.try_pop() {
		match command {
			Command::Play(event) => arrived.push(event),
			_ => panic!("a played note is not a service message"),
		}
	}
	assert_eq!(
		arrived,
		played[..2],
		"what does not fit the ring is dropped, the rest keeps its order"
	);
}

#[test]
fn effects_settings_round_trip_through_the_config_and_clamp_levels() {
	let mut synth = SynthConfig {
		reverb: false,
		chorus: true,
		reverb_level: 200,
		chorus_level: 7,
		..SynthConfig::default()
	};
	let settings = EffectsSettings::from_config(&synth);
	assert_eq!(
		settings,
		EffectsSettings {
			reverb: false,
			chorus: true,
			reverb_level: 127,
			chorus_level: 7
		}
	);
	EffectsSettings {
		reverb: true,
		chorus: false,
		reverb_level: 40,
		chorus_level: 0,
	}
	.apply_to(&mut synth);
	assert!(synth.reverb && !synth.chorus);
	assert_eq!((synth.reverb_level, synth.chorus_level), (40, 0));
	assert_eq!(synth.sf2, None, "other synth fields untouched");
}

/// The ring of samples survives a take that did not start (the defect behind plan step 6.1: one failed
/// start used to leave the session unable to record until the audio stream was reopened).
#[test]
fn a_failed_start_keeps_the_ring_for_the_next_take() {
	let params = RecordParams {
		sample_rate: 48_000,
		vorbis_quality: 0.5,
	};
	let mut ring = None;
	assert!(
		begin_take(&mut ring, RecordFormat::Flac, PathBuf::from("take.flac"), params).is_none(),
		"nothing to record from"
	);
	assert!(!Path::new("take.flac").exists(), "no file is created without a ring");

	let (_producer, consumer) = HeapRb::<f32>::new(64).split();
	ring = Some(consumer);
	let nowhere = PathBuf::from("definitely/not/here/take.flac");
	let failed = begin_take(&mut ring, RecordFormat::Flac, nowhere, params).expect("there was a ring");
	assert!(matches!(failed, Err(RecordError::Create { .. })));
	assert!(ring.is_some(), "the ring is back after the failure");

	let path = std::env::temp_dir().join(format!("sf2_player-begin-take-{}.flac", std::process::id()));
	let recorder = begin_take(&mut ring, RecordFormat::Flac, path.clone(), params)
		.expect("there was a ring")
		.expect("the second attempt starts");
	assert!(ring.is_none(), "the ring is lent to the running take");
	let (returned, removed) = recorder.cancel();
	assert!(returned.is_some() && removed.is_ok());
	assert!(!path.exists());
}
