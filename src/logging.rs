//! File logging via `tracing` (spec N-07): nothing goes to stdout, the TUI owns the terminal.

use std::path::PathBuf;

use anyhow::Context;
use directories::ProjectDirs;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;

const APP_NAME: &str = "sf2_player";
const LOG_FILE_NAME: &str = "sf2_player.log";

/// Keeps the background log writer alive; dropping it flushes pending lines to the file.
pub type LogGuard = WorkerGuard;

/// Installs the global subscriber writing to `<OS local data dir>/sf2_player/sf2_player.log`.
pub fn init(level: LevelFilter) -> anyhow::Result<LogGuard> {
	let dir = log_dir()?;
	std::fs::create_dir_all(&dir).with_context(|| format!("cannot create log directory {}", dir.display()))?;
	let file_appender = tracing_appender::rolling::never(&dir, LOG_FILE_NAME);
	let (writer, guard) = tracing_appender::non_blocking(file_appender);
	tracing_subscriber::fmt()
		.with_writer(writer)
		.with_ansi(false)
		.with_max_level(level)
		.init();
	Ok(guard)
}

fn log_dir() -> anyhow::Result<PathBuf> {
	let dirs = ProjectDirs::from("", "", APP_NAME).context("cannot determine the OS application directories")?;
	Ok(dirs.data_local_dir().to_path_buf())
}
