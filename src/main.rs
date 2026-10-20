use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use ratatui::crossterm::event::{
	DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::supports_keyboard_enhancement;
use sf2_player::cli::Cli;
use sf2_player::config::{self, Config, Settings};
use sf2_player::session::Session;
use sf2_player::ui::text_keyboard::{NoteOffMode, TextKeyboard};
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
	let kitty = supports_keyboard_enhancement().unwrap_or(false);
	// The Windows console reports key releases natively; elsewhere the kitty keyboard protocol is needed (F-16).
	let release_reported = cfg!(windows) || kitty;
	let mode = if release_reported && !session.config.keyboard.force_gate {
		NoteOffMode::Release
	} else {
		NoteOffMode::Gate(std::time::Duration::from_millis(u64::from(session.config.keyboard.gate_ms)))
	};
	tracing::info!(kitty, release_reported, ?mode, "text keyboard mode");
	let mut app = App::with_keyboard(locale, TextKeyboard::new(&session.config.keyboard, mode));

	// Nothing may print to stdout from here until `restore` (spec N-07); ratatui's init installs the panic hook (N-08).
	let mut terminal = ratatui::init();
	let paste = execute!(std::io::stdout(), EnableBracketedPaste);
	let flags = kitty
		&& execute!(
			std::io::stdout(),
			PushKeyboardEnhancementFlags(
				KeyboardEnhancementFlags::REPORT_EVENT_TYPES | KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
			)
		)
		.is_ok();
	let result = ui::run(&mut terminal, &mut app, &mut session);
	if flags {
		let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
	}
	if paste.is_ok() {
		let _ = execute!(std::io::stdout(), DisableBracketedPaste);
	}
	ratatui::restore();
	result?;
	session.save_config();
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
