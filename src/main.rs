use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use sf2_player::cli::Cli;
use sf2_player::config::{self, Config, Settings};
use sf2_player::session::Session;
use sf2_player::ui::{self, App};
use sf2_player::{audio, logging, midi};

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
	let locale = ui::i18n::resolve_locale(config.ui.lang.as_deref(), ui::i18n::system_locale().as_deref());
	tracing::info!(locale, "ui language");

	let mut session = Session::start(&settings, config, config_path);
	let mut app = App::new(locale);

	// Nothing may print to stdout from here until `restore` (spec N-07); ratatui's init installs the panic hook (N-08).
	let mut terminal = ratatui::init();
	let result = ui::run(&mut terminal, &mut app, &mut session);
	ratatui::restore();
	result?;
	tracing::info!(xruns = session.shared.xruns(), "sf2_player exiting");
	Ok(())
}

fn resolve_config_path(cli: &Cli) -> anyhow::Result<PathBuf> {
	match &cli.config {
		Some(path) => Ok(path.clone()),
		None => config::default_config_path().context("cannot determine the OS config directory"),
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
