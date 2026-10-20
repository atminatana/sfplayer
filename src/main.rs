use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::Context;
use clap::Parser;
use cpal::BufferSize;
use ringbuf::traits::Producer;
use sf2_player::audio::{self, AudioShared, StreamParams};
use sf2_player::cli::{Cli, DeviceSelector};
use sf2_player::logging;
use sf2_player::midi::{self, MidiInputHandle};
use sf2_player::synth::{RustySynthEngine, SoundFontBank, SynthEngine};

fn main() -> anyhow::Result<()> {
	let cli = Cli::parse();
	let _log_guard = logging::init(cli.log_level)?;
	tracing::info!(version = env!("CARGO_PKG_VERSION"), "sf2_player starting");
	tracing::debug!(?cli, "command line");

	if cli.list_devices {
		return print_devices();
	}
	let sf2_path = cli.sf2.as_deref().context("no SoundFont given: pass --sf2 PATH")?;
	let bank = SoundFontBank::load(sf2_path)?;
	tracing::info!(name = bank.name(), presets = bank.presets().len(), "SoundFont loaded");

	let device = audio::default_output_device()?;
	let shared = Arc::new(AudioShared::default());
	let params = StreamParams {
		buffer_frames: cli.buffer,
		sample_rate: cli.sample_rate,
	};
	let make_engine = |sample_rate: u32| -> Result<Box<dyn SynthEngine>, _> { Ok(Box::new(RustySynthEngine::new(&bank, sample_rate)?)) };
	let (stream, mut events) = audio::open_output_stream(&device, params, &make_engine, &shared)?;

	let dropped_events = Arc::new(AtomicU32::new(0));
	let dropped_counter = Arc::clone(&dropped_events);
	let midi_handle = match pick_midi_port(cli.midi_in.as_ref())? {
		Some(index) => Some(midi::connect_input(index, move |event| {
			if events.try_push(event).is_err() {
				dropped_counter.fetch_add(1, Ordering::Relaxed);
			}
		})?),
		None => None,
	};

	let buffer = match stream.buffer_size {
		BufferSize::Fixed(frames) => frames.to_string(),
		BufferSize::Default => "default".to_owned(),
	};
	let midi_name = midi_handle
		.as_ref()
		.map_or("none", |handle: &MidiInputHandle| handle.port_name.as_str());
	println!(
		"Playing {} on {} @ {} Hz, buffer {buffer}.",
		bank.name(),
		stream.device_name,
		stream.sample_rate
	);
	println!("MIDI in: {midi_name}. Press Enter to stop.");
	std::io::stdout().flush()?;
	std::io::stdin().lock().lines().next();
	println!(
		"xruns: {}, dropped MIDI events: {}",
		shared.xruns(),
		dropped_events.load(Ordering::Relaxed)
	);
	Ok(())
}

/// Resolves `--midi-in`; without it, the first available port is used; with no ports at all, MIDI is skipped.
fn pick_midi_port(selector: Option<&DeviceSelector>) -> anyhow::Result<Option<usize>> {
	let ports = midi::list_input_ports()?;
	let names = ports.iter().map(|port| port.name.as_str());
	match selector {
		Some(selector) => selector
			.find(names)
			.map(Some)
			.with_context(|| format!("MIDI input {selector:?} not found; see --list-devices")),
		None => Ok(ports.first().map(|port| port.index)),
	}
}

fn print_devices() -> anyhow::Result<()> {
	println!("MIDI inputs:");
	for port in midi::list_input_ports()? {
		println!("  [{}] {}", port.index, port.name);
	}
	println!("Audio outputs:");
	for device in audio::list_output_devices()? {
		let marker = if device.is_default { "*" } else { " " };
		let buffer = device.buffer_range.map_or("buffer range unknown".to_owned(), |(min, max)| {
			format!("buffer {min}..{max} frames")
		});
		println!(
			"{marker} [{}] {}  {} Hz, {buffer}",
			device.index, device.name, device.default_sample_rate
		);
	}
	Ok(())
}
