//! Takes of the session: starting, stopping and cancelling a recording, and where its files go.

use std::path::PathBuf;

use super::{Notice, Session, control};
use crate::audio::RecordMode;
use crate::record::{RecordParams, Recorder, file_name};

impl Session {
	/// Home: starts a recording in the configured format (F-20, F-23); a running take is left alone.
	/// Whatever goes wrong, the recording rings stay with the session for the next attempt.
	pub fn start_recording(&mut self) {
		if self.recorder.is_some() {
			return;
		}
		let Some(sample_rate) = self.audio.as_ref().map(|stream| stream.sample_rate) else {
			return;
		};
		let dir = self.record_dir();
		if let Err(error) = std::fs::create_dir_all(&dir) {
			self.notice = Some(Notice::RecordError(error.to_string()));
			return;
		}
		let format = self.config.record.format;
		let path = file_name(&dir, format, chrono::Local::now());
		let params = RecordParams {
			sample_rate,
			vorbis_quality: self.config.record.vorbis_quality,
		};
		match control::begin_take(&mut self.record_rx, format, path, params) {
			Some(Ok(recorder)) => {
				self.shared.set_recording(control::record_mode(format));
				self.recorder = Some(recorder);
			}
			Some(Err(error)) => {
				tracing::warn!(%error, "recording start failed");
				self.notice = Some(Notice::RecordError(error.to_string()));
			}
			None => {}
		}
	}

	/// End (and quit): stops the recording if one is running; the file name or the error goes to the status line.
	pub fn stop_recording(&mut self) {
		let Some(recorder) = self.recorder.take() else {
			return;
		};
		self.shared.set_recording(RecordMode::Off);
		let (rings, result) = recorder.stop();
		self.record_rx = rings;
		self.notice = Some(match result {
			Ok(take) => {
				let name = take
					.path
					.file_name()
					.map(|name| name.to_string_lossy().into_owned())
					.unwrap_or_default();
				self.last_take = Some(take);
				Notice::RecordSaved(name)
			}
			Err(error) => Notice::RecordError(error.to_string()),
		});
	}

	/// The Cancel button (spec v1.2, F-23): the take is stopped and its file removed.
	pub fn cancel_recording(&mut self) {
		let Some(recorder) = self.recorder.take() else {
			return;
		};
		self.shared.set_recording(RecordMode::Off);
		let (rings, removed) = recorder.cancel();
		self.record_rx = rings;
		self.notice = Some(match removed {
			Ok(()) => Notice::RecordCancelled,
			Err(error) => Notice::RecordError(error.to_string()),
		});
	}

	/// The file of the running take: its name and its size so far.
	pub fn recording_file(&self) -> Option<(String, u64)> {
		let recorder = self.recorder.as_ref()?;
		let name = recorder
			.path()
			.file_name()
			.map(|name| name.to_string_lossy().into_owned())
			.unwrap_or_default();
		Some((name, recorder.bytes()))
	}

	/// Elapsed time of the running recording, if any.
	pub fn recording_elapsed(&self) -> Option<std::time::Duration> {
		self.recorder.as_ref().map(Recorder::elapsed)
	}

	/// `record.dir` from the config, else the user's Music directory, else the app data directory.
	pub fn record_dir(&self) -> PathBuf {
		if let Some(dir) = &self.config.record.dir {
			return dir.clone();
		}
		directories::UserDirs::new()
			.and_then(|dirs| dirs.audio_dir().map(|dir| dir.join("sf2_player")))
			.or_else(|| crate::config::project_dirs().map(|dirs| dirs.data_dir().join("recordings")))
			.unwrap_or_else(|| PathBuf::from("recordings"))
	}
}
