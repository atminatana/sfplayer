use std::io::{BufRead, Write};
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use cpal::BufferSize;
use sf2_player::audio::{self, AudioShared, StreamParams};
use sf2_player::cli::Cli;
use sf2_player::logging;
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
	let (stream, _events) = audio::open_output_stream(&device, params, &make_engine, &shared)?;

	let buffer = match stream.buffer_size {
		BufferSize::Fixed(frames) => frames.to_string(),
		BufferSize::Default => "default".to_owned(),
	};
	println!(
		"Playing {} on {} @ {} Hz, buffer {buffer}. Press Enter to stop.",
		bank.name(),
		stream.device_name,
		stream.sample_rate
	);
	std::io::stdout().flush()?;
	std::io::stdin().lock().lines().next();
	println!("xruns: {}", shared.xruns());
	Ok(())
}

fn print_devices() -> anyhow::Result<()> {
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
