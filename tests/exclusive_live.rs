//! Live check of the WASAPI Exclusive driver (spec v1.4, К-5): every output of the machine is opened
//! exclusively and played for half a second with the built-in synthesizer holding a note at zero
//! volume, so the whole path runs and nothing is heard.
//!
//! Not a part of the usual run: it needs devices, and while a device is taken no other program
//! sounds through it. Run it with
//! `cargo test --test exclusive_live -- --ignored --nocapture`.

#![cfg(windows)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use ringbuf::traits::Producer;
use sf2_player::audio::{AudioShared, Command, Driver, StreamParams, list_output_devices, open_output_stream, output_device_by_index};
use sf2_player::midi::MidiEvent;
use sf2_player::synth::{Adsr, BuiltinSynth, Timbre, Wave};

const BUFFER_FRAMES: u32 = 256;
const PLAY_TIME: Duration = Duration::from_millis(500);
/// How long a device may take to ask for its first block: a Bluetooth link is slow to come up.
const START_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

#[test]
#[ignore = "takes every output device exclusively: needs devices and silences other programs meanwhile"]
fn every_output_opens_exclusively_or_says_why_not() {
	let outputs = list_output_devices().expect("the outputs can be listed");
	let exclusive: Vec<_> = outputs.iter().filter(|output| output.driver == Driver::WasapiExclusive).collect();
	assert!(!exclusive.is_empty(), "the list has no exclusive output: {outputs:?}");
	let mut opened = 0;
	for info in exclusive {
		let shared = Arc::new(AudioShared::new(0.0));
		let params = StreamParams {
			buffer_frames: BUFFER_FRAMES,
			sample_rate: None,
		};
		let device = output_device_by_index(info.index).expect("the output is still there");
		match open_output_stream(&device, params, &shared) {
			Ok((stream, mut links)) => {
				let adsr = Adsr {
					attack: 0.003,
					decay: 0.5,
					sustain: 0.35,
					release: 0.35,
				};
				let engine = BuiltinSynth::new(stream.sample_rate, Arc::new(Timbre::new(Wave::Sine, adsr)));
				let note = MidiEvent::NoteOn {
					channel: 0,
					key: 60,
					velocity: 100,
				};
				assert!(links.commands.try_push(Command::ReplaceEngine(Some(Box::new(engine)))).is_ok());
				assert!(links.commands.try_push(Command::Play(note)).is_ok());
				// The note is counted by the first block the device asks for.
				let asked = Instant::now();
				while shared.held_notes() == 0 && asked.elapsed() < START_TIMEOUT {
					std::thread::sleep(POLL);
				}
				let started = asked.elapsed();
				std::thread::sleep(PLAY_TIME);
				let frames = stream.buffer_frames.expect("an exclusive stream knows its period");
				println!(
					"{}: {} Hz, period {frames} frames = {:.2} ms, first block after {} ms, held notes {}, xruns {}",
					stream.label,
					stream.sample_rate,
					f64::from(frames) * 1000.0 / f64::from(stream.sample_rate),
					started.as_millis(),
					shared.held_notes(),
					shared.xruns(),
				);
				assert_eq!(stream.label, info.label());
				assert_eq!(shared.held_notes(), 1, "the audio thread took the note");
				opened += 1;
			}
			Err(error) => println!("{}: NOT OPENED: {error}", info.label()),
		}
	}
	assert!(opened > 0, "no output could be opened exclusively");
}
