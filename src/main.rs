use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use sf2_player::cli::Cli;
use sf2_player::config::{self, Config, Settings};
use sf2_player::session::Session;
use sf2_player::ui::keys::Bindings;
use sf2_player::ui::text_keyboard::TextKeyboard;
use sf2_player::ui::window::{self, WindowOptions};
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

	let session = Session::start(&settings, config, config_path);
	// A mark between the devices and the window: a slow start shows in the log on which side it was slow.
	tracing::debug!("session started");
	log_panics();
	let mut app = App::with_keyboard(locale, TextKeyboard::new(&session.config.keyboard));
	// Key overrides are validated before the window opens, so a bad [keys] entry is a plain error (ADR 0007).
	app.bindings = Bindings::from_config(&session.config.keys, &session.config.keyboard).context("invalid key configuration")?;
	let options = WindowOptions {
		font: session.config.ui.font.clone(),
		font_size: session.config.ui.font_size,
		backend: cli.backend.clone(),
	};
	// Nothing prints to stdout while the window is open (spec N-07); the window saves the config and
	// finishes a running take when it closes.
	window::run(app, session, &options)
}

/// Sends a panic to the log before the default report: with a window there may be no console to read it from.
fn log_panics() {
	let default_hook = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |panic| {
		tracing::error!(%panic, "panic");
		default_hook(panic);
	}));
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
