use clap::Parser;
use sf2_player::cli::Cli;
use sf2_player::logging;
use sf2_player::synth::SoundFontBank;

fn main() -> anyhow::Result<()> {
	let cli = Cli::parse();
	let _log_guard = logging::init(cli.log_level)?;
	tracing::info!(version = env!("CARGO_PKG_VERSION"), "sf2_player starting");
	tracing::debug!(?cli, "command line");

	if let Some(path) = &cli.sf2 {
		let bank = SoundFontBank::load(path)?;
		tracing::info!(name = bank.name(), presets = bank.presets().len(), "SoundFont loaded");
	}
	Ok(())
}
