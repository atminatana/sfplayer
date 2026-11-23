//! Live check of the ASIO driver (spec v1.4, К-9): every ASIO output of the machine opens a stream in
//! the sample format of its driver and plays half a second with the built-in synthesizer holding a
//! note at zero volume.
//!
//! Built only with the `asio` feature, and not a part of the usual run: it needs an installed ASIO
//! driver that nothing else is using. Run it with
//! `cargo test --features asio --test asio_live -- --ignored --nocapture`.

#![cfg(all(windows, feature = "asio"))]

use std::sync::Arc;
use std::time::{Duration, Instant};

use ringbuf::traits::Producer;
use sf2_player::audio::{AudioShared, Command, Driver, StreamParams, list_output_devices, open_output_stream, output_device_by_index};
use sf2_player::midi::MidiEvent;
use sf2_player::synth::{Adsr, BuiltinSynth, Timbre, Wave};

const BUFFER_FRAMES: u32 = 256;
const PLAY_TIME: Duration = Duration::from_millis(500);
const START_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

#[test]
#[ignore = "needs an installed ASIO driver that nothing else is using"]
fn every_asio_driver_opens_a_stream_in_its_own_format() {
	let outputs = list_output_devices().expect("the outputs can be listed");
	let drivers: Vec<_> = outputs.iter().filter(|output| output.driver == Driver::Asio).collect();
	assert!(!drivers.is_empty(), "the list has no ASIO output: {outputs:?}");
	for info in drivers {
		assert!(info.label().starts_with("[ASIO]: "), "{}", info.label());
		println!(
			"{}: {} Hz, buffer range {:?}",
			info.label(),
			info.default_sample_rate,
			info.buffer_range
		);
		let shared = Arc::new(AudioShared::new(0.0));
		let params = StreamParams {
			buffer_frames: BUFFER_FRAMES,
			sample_rate: None,
		};
		let device = output_device_by_index(info.index).expect("the output is still there");
		let (stream, mut links) = open_output_stream(&device, params, &shared).expect("the ASIO stream opens");
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
		let asked = Instant::now();
		while shared.held_notes() == 0 && asked.elapsed() < START_TIMEOUT {
			std::thread::sleep(POLL);
		}
		let started = asked.elapsed();
		std::thread::sleep(PLAY_TIME);
		println!(
			"{}: {} Hz, callback {:?} frames, first block after {} ms, held notes {}",
			stream.label,
			stream.sample_rate,
			stream.buffer_frames,
			started.as_millis(),
			shared.held_notes(),
		);
		assert_eq!(stream.label, info.label());
		assert_eq!(shared.held_notes(), 1, "the audio thread took the note");
	}
}
