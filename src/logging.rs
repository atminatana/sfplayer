//! File logging via `tracing` (spec N-07): nothing goes to stdout, the TUI owns the terminal.

use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;

use crate::config::project_dirs;

const LOG_FILE_NAME: &str = "sf2_player.log";

/// Keeps the background log writer alive; dropping it flushes pending lines to the file.
pub type LogGuard = WorkerGuard;

/// Installs the global subscriber writing to `<OS local data dir>/sf2_player/sf2_player.log`.
/// A log left by a previous run is removed first, so each session starts with a clean file.
pub fn init(level: LevelFilter) -> anyhow::Result<LogGuard> {
	let dir = log_dir()?;
	std::fs::create_dir_all(&dir).with_context(|| format!("cannot create log directory {}", dir.display()))?;
	// Deletion must precede `rolling::never`, which opens (and keeps open) the file it writes to.
	let stale_log_error = remove_previous_log(&dir);
	let file_appender = tracing_appender::rolling::never(&dir, LOG_FILE_NAME);
	let (writer, guard) = tracing_appender::non_blocking(file_appender);
	tracing_subscriber::fmt()
		.with_writer(writer)
		.with_ansi(false)
		.with_max_level(level)
		.init();
	// Only now a `tracing` call reaches the log; before `.init()` it would be dropped.
	if let Some(error) = stale_log_error {
		tracing::warn!(%error, path = %dir.join(LOG_FILE_NAME).display(), "cannot remove the previous log");
	}
	Ok(guard)
}

fn log_dir() -> anyhow::Result<PathBuf> {
	let dirs = project_dirs().context("cannot determine the OS application directories")?;
	Ok(dirs.data_local_dir().to_path_buf())
}

/// Removes a previous run's log. `NotFound` is the expected first run; any other failure is
/// returned so `init` can report it — a leftover log only means the new session appends to it,
/// which is not worth failing the startup.
fn remove_previous_log(dir: &Path) -> Option<std::io::Error> {
	let path = dir.join(LOG_FILE_NAME);
	match std::fs::remove_file(&path) {
		Ok(()) => None,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
		Err(error) => Some(error),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A scratch directory per test; removed on drop.
	fn scratch_dir(name: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("sf2_player_log_test_{name}_{}", std::process::id()));
		std::fs::create_dir_all(&dir).expect("create scratch dir");
		dir
	}

	#[test]
	fn removing_a_missing_log_is_not_an_error() {
		let dir = scratch_dir("missing");
		assert!(remove_previous_log(&dir).is_none());
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn removing_an_existing_log_deletes_it_and_reports_ok() {
		let dir = scratch_dir("existing");
		let path = dir.join(LOG_FILE_NAME);
		std::fs::write(&path, "leftover from a previous run").expect("write scratch log");
		assert!(remove_previous_log(&dir).is_none());
		assert!(!path.exists(), "the previous log should be gone");
		let _ = std::fs::remove_dir_all(&dir);
	}
}
