//! Live checks of the WASAPI Exclusive driver (spec v1.4, К-5, К-8): every output of the machine is
//! opened exclusively and played for half a second with the built-in synthesizer holding a note at
//! zero volume, so the whole path runs and nothing is heard; and a session whose output is taken by
//! somebody else starts on another one.
//!
//! Not a part of the usual run: it needs devices, and while a device is taken no other program
//! sounds through it. Run it with
//! `cargo test --test exclusive_live -- --ignored --nocapture`.

#![cfg(windows)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use ringbuf::traits::Producer;
use sf2_player::audio::{AudioShared, Command, Driver, StreamParams, list_output_devices, open_output_stream, output_device_by_index};
use sf2_player::cli::DeviceSelector;
use sf2_player::config::{Config, Settings};
use sf2_player::midi::MidiEvent;
use sf2_player::session::{Notice, Session};
use sf2_player::synth::{Adsr, BuiltinSynth, Timbre, Wave};

const BUFFER_FRAMES: u32 = 256;
const PLAY_TIME: Duration = Duration::from_millis(500);
/// How long a device may take to ask for its first block: a Bluetooth link is slow to come up.
const START_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

#[test]
#[ignore = "takes every output device exclusively: needs devices and silences other programs meanwhile"]
fn every_output_opens_exclusively_or_says_why_not() {
	// The log line of an opened stream names its sample format: with `--nocapture` it is printed here.
	// Another test of this binary may have installed the subscriber already.
	let _ = tracing_subscriber::fmt().with_ansi(false).try_init();
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

#[test]
#[ignore = "takes an output device exclusively: needs two devices and silences other programs meanwhile"]
fn a_session_whose_output_is_in_use_starts_on_another_one() {
	let outputs = list_output_devices().expect("the outputs can be listed");
	let busy = outputs
		.iter()
		.find(|output| output.driver == Driver::WasapiExclusive)
		.expect("the list has an exclusive output");
	let params = StreamParams {
		buffer_frames: BUFFER_FRAMES,
		sample_rate: None,
	};
	let device = output_device_by_index(busy.index).expect("the output is still there");
	// Somebody else has the device: neither an exclusive nor a shared stream opens on it now.
	let _holder = open_output_stream(&device, params, &Arc::new(AudioShared::new(0.0))).expect("the output opens exclusively");

	let settings = Settings {
		sf2: None,
		midi_in: None,
		audio_out: Some(DeviceSelector::Name(busy.label())),
		buffer_frames: BUFFER_FRAMES,
		sample_rate: None,
	};
	let mut config = Config::default();
	config.mixer.master = 0;
	let session = Session::start(&settings, config, std::env::temp_dir().join("sf2_player-exclusive-live.toml"));

	let playing = session.audio().map(|stream| stream.label.clone());
	println!("wanted {}, playing {playing:?}, notice {:?}", busy.label(), session.notice);
	let playing = playing.expect("another output plays");
	assert_ne!(playing, busy.label());
	assert!(
		matches!(&session.notice, Some(Notice::AudioFellBack(error)) if error.contains(&busy.label())),
		"{:?}",
		session.notice
	);
}

/// Buffer sizes asked for in the measurement; one frame asks for the shortest period of the device.
const MEASURED_BUFFERS: [u32; 5] = [1, 128, 256, 512, 1024];
const MEASURE_TIME: Duration = Duration::from_secs(2);

/// The numbers of spec v1.4, К-6: for every exclusive output and several buffer sizes — the period the
/// device agreed to, the latency its driver states, and how many wake-ups came late in two seconds.
#[test]
#[ignore = "takes every output device exclusively for some ten seconds: silences other programs meanwhile"]
fn the_period_the_latency_and_the_late_wake_ups_of_every_output() {
	let outputs = list_output_devices().expect("the outputs can be listed");
	for info in outputs.iter().filter(|output| output.driver == Driver::WasapiExclusive) {
		for buffer_frames in MEASURED_BUFFERS {
			let shared = Arc::new(AudioShared::new(0.0));
			let params = StreamParams {
				buffer_frames,
				sample_rate: None,
			};
			let device = output_device_by_index(info.index).expect("the output is still there");
			let (stream, _links) = match open_output_stream(&device, params, &shared) {
				Ok(opened) => opened,
				Err(error) => {
					println!("{} | asked {buffer_frames} | NOT OPENED: {error}", info.label());
					continue;
				}
			};
			std::thread::sleep(MEASURE_TIME);
			let frames = stream.buffer_frames.expect("an exclusive stream knows its period");
			let period_ms = f64::from(frames) * 1000.0 / f64::from(stream.sample_rate);
			let stated_ms = stream.latency.expect("an exclusive stream states its latency").as_secs_f64() * 1000.0;
			println!(
				"{} | {} Hz | asked {buffer_frames} | period {frames} frames = {period_ms:.2} ms | stated latency {stated_ms:.2} ms | late wake-ups {}",
				stream.label,
				stream.sample_rate,
				shared.xruns(),
			);
		}
	}
}
