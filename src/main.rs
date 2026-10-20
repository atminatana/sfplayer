use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::Context;
use clap::Parser;
use cpal::BufferSize;
use ringbuf::traits::Producer;
use sf2_player::audio::{self, AudioShared, StreamParams};
use sf2_player::cli::{Cli, DeviceSelector};
use sf2_player::config::{self, Config, Settings};
use sf2_player::logging;
use sf2_player::midi::{self, DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiInputHandle, MidiStatus};
use sf2_player::synth::{RustySynthEngine, SoundFontBank, SynthEngine};

fn main() -> anyhow::Result<()> {
	let cli = Cli::parse();
	let _log_guard = logging::init(cli.log_level)?;
	tracing::info!(version = env!("CARGO_PKG_VERSION"), "sf2_player starting");
	tracing::debug!(?cli, "command line");

	if cli.list_devices {
		return print_devices();
	}
	let config_path = resolve_config_path(&cli)?;
	let config = match &cli.config {
		Some(_) => Config::load(&config_path)?,
		None => Config::load_or_default(&config_path)?,
	};
	tracing::info!(path = %config_path.display(), "config loaded");
	let settings = Settings::resolve(&cli, &config)?;

	let sf2_path = settings
		.sf2
		.as_deref()
		.context("no SoundFont given: pass --sf2 PATH or set synth.sf2 in the config")?;
	let bank = SoundFontBank::load(sf2_path)?;
	tracing::info!(name = bank.name(), presets = bank.presets().len(), "SoundFont loaded");

	let selected = audio::select_output_device(settings.audio_out.as_ref())?;
	if selected.requested_missing {
		println!("Audio output {:?} not found, using the default device.", settings.audio_out);
	}
	let device = selected.device;
	let shared = Arc::new(AudioShared::default());
	let params = StreamParams {
		buffer_frames: settings.buffer_frames,
		sample_rate: settings.sample_rate,
	};
	let make_engine = |sample_rate: u32| -> Result<Box<dyn SynthEngine>, _> { Ok(Box::new(RustySynthEngine::new(&bank, sample_rate)?)) };
	let (stream, mut events) = audio::open_output_stream(&device, params, &make_engine, &shared)?;

	let dropped_events = Arc::new(AtomicU32::new(0));
	let dropped_counter = Arc::clone(&dropped_events);
	let (midi_index, midi_missing) = pick_midi_port(settings.midi_in.as_ref())?;
	if midi_missing {
		println!("MIDI input {:?} not found, using the first available port.", settings.midi_in);
	}
	let midi_ports = midi::list_input_ports()?;
	let midi_handle = match midi_index {
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
	let connected_port = midi_handle.as_ref().map(|handle| handle.port_name.clone());
	let _hotplug = HotplugWatcher::spawn(DEFAULT_POLL_INTERVAL, midi_ports, move |ports| {
		let status = MidiStatus::from_ports(connected_port.as_deref(), &ports);
		tracing::info!(?status, ports = ports.len(), "MIDI ports changed");
		println!("MIDI ports changed ({} available), status: {status:?}", ports.len());
	});
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

fn resolve_config_path(cli: &Cli) -> anyhow::Result<PathBuf> {
	match &cli.config {
		Some(path) => Ok(path.clone()),
		None => config::default_config_path().context("cannot determine the OS config directory"),
	}
}

/// Resolves the MIDI selector to a port index plus a flag telling that the request matched nothing.
/// Without a match the first available port is used; with no ports at all, MIDI is skipped.
fn pick_midi_port(selector: Option<&DeviceSelector>) -> anyhow::Result<(Option<usize>, bool)> {
	let ports = midi::list_input_ports()?;
	let first = ports.first().map(|port| port.index);
	let Some(selector) = selector else {
		return Ok((first, false));
	};
	match selector.find(ports.iter().map(|port| port.name.as_str())) {
		Some(index) => Ok((Some(index), false)),
		None => {
			tracing::warn!(?selector, "MIDI input not found, using the first available port");
			Ok((first, true))
		}
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
