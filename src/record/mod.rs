//! Recording the output (spec F-20, ADR 0005): the audio callback drops interleaved f32 frames into an SPSC
//! ring; this module's thread drains the ring and encodes to FLAC or Ogg Vorbis. Nothing here runs on the
//! audio thread.

mod flac;
mod vorbis;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ringbuf::HeapCons;
use ringbuf::traits::Consumer;

use crate::config::RecordFormat;

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
	#[error("cannot create {path}")]
	Create {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("cannot write {path}")]
	Write {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("FLAC encoder: {0}")]
	Flac(String),
	#[error("Ogg Vorbis encoder: {0}")]
	Vorbis(String),
	#[error("recorder thread panicked")]
	ThreadPanicked,
}

/// One running encoder behind a common interface. Not `Send`: libvorbis state holds raw pointers, so the
/// encoder is created and dropped on the recorder thread.
trait Encoder {
	/// Interleaved stereo frames, any length.
	fn write_block(&mut self, interleaved: &[f32]) -> Result<(), RecordError>;
	fn finish(self: Box<Self>) -> Result<(), RecordError>;
}

/// Stereo only: the engine renders two channels (spec F-04).
pub const CHANNELS: usize = 2;
/// How often the thread looks at the ring; the ring holds seconds, so this is generous.
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const DRAIN_CHUNK_SAMPLES: usize = 8192;

pub fn file_extension(format: RecordFormat) -> &'static str {
	match format {
		RecordFormat::Flac => "flac",
		RecordFormat::OggVorbis => "ogg",
	}
}

/// "2026-10-20_12-03-41.flac" in `dir` (spec F-20).
pub fn file_name(dir: &Path, format: RecordFormat, now: chrono::DateTime<chrono::Local>) -> PathBuf {
	dir.join(format!("{}.{}", now.format("%Y-%m-%d_%H-%M-%S"), file_extension(format)))
}

/// Encoder parameters that do not depend on the format.
#[derive(Debug, Clone, Copy)]
pub struct RecordParams {
	pub sample_rate: u32,
	/// Ogg Vorbis quality 0.0..=1.0 (ignored for FLAC).
	pub vorbis_quality: f32,
}

fn open_encoder(format: RecordFormat, path: &Path, params: RecordParams) -> Result<Box<dyn Encoder>, RecordError> {
	Ok(match format {
		RecordFormat::Flac => Box::new(flac::FlacFile::create(path, params.sample_rate)?),
		RecordFormat::OggVorbis => Box::new(vorbis::VorbisFile::create(path, params.sample_rate, params.vorbis_quality)?),
	})
}

/// What the recorder thread hands back: the ring consumer and the encoding outcome.
type TakeOutcome = (HeapCons<f32>, Result<(), RecordError>);

/// A recording in progress. `stop` finalises the file and hands the ring consumer back for the next take.
pub struct Recorder {
	stop: Option<Sender<()>>,
	thread: Option<JoinHandle<TakeOutcome>>,
	path: PathBuf,
	started: Instant,
}

impl Recorder {
	/// Creates the file (on the recorder thread, reporting back before returning), then keeps draining `samples`.
	pub fn start(format: RecordFormat, path: PathBuf, params: RecordParams, mut samples: HeapCons<f32>) -> Result<Self, RecordError> {
		// Drop whatever was sitting in the ring before the user pressed record.
		while samples.try_pop().is_some() {}
		let (stop, stop_signal) = mpsc::channel::<()>();
		let (opened_tx, opened_rx) = mpsc::channel::<Result<(), RecordError>>();
		let thread_path = path.clone();
		let thread = std::thread::Builder::new()
			.name("recorder".to_owned())
			.spawn(move || {
				let mut encoder = match open_encoder(format, &thread_path, params) {
					Ok(encoder) => {
						let _ = opened_tx.send(Ok(()));
						encoder
					}
					Err(error) => {
						let _ = opened_tx.send(Err(error));
						return (samples, Err(RecordError::ThreadPanicked));
					}
				};
				let mut chunk = vec![0.0f32; DRAIN_CHUNK_SAMPLES];
				let result = loop {
					let finishing = !matches!(stop_signal.recv_timeout(POLL_INTERVAL), Err(RecvTimeoutError::Timeout));
					if let Err(error) = drain(&mut samples, &mut chunk, encoder.as_mut()) {
						break Err(error);
					}
					if finishing {
						break encoder.finish();
					}
				};
				(samples, result)
			})
			.map_err(|source| RecordError::Create {
				path: path.clone(),
				source,
			})?;
		match opened_rx.recv() {
			Ok(Ok(())) => {}
			Ok(Err(error)) => {
				// The thread has already returned; its join only recovers the ring consumer, which the caller
				// cannot reuse through this error path, so the stream owner re-creates the ring on reopen.
				let _ = thread.join();
				return Err(error);
			}
			Err(_) => return Err(RecordError::ThreadPanicked),
		}
		tracing::info!(path = %path.display(), ?format, "recording started");
		Ok(Self {
			stop: Some(stop),
			thread: Some(thread),
			path,
			started: Instant::now(),
		})
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	pub fn elapsed(&self) -> Duration {
		self.started.elapsed()
	}

	/// Stops, finalises the file and returns the ring consumer plus the outcome.
	pub fn stop(mut self) -> (Option<HeapCons<f32>>, Result<PathBuf, RecordError>) {
		drop(self.stop.take());
		match self.thread.take().map(JoinHandle::join) {
			Some(Ok((samples, Ok(())))) => {
				tracing::info!(path = %self.path.display(), "recording saved");
				(Some(samples), Ok(self.path.clone()))
			}
			Some(Ok((samples, Err(error)))) => (Some(samples), Err(error)),
			Some(Err(_)) | None => (None, Err(RecordError::ThreadPanicked)),
		}
	}
}

/// Moves everything currently in the ring into the encoder, whole frames at a time.
fn drain(samples: &mut HeapCons<f32>, chunk: &mut [f32], encoder: &mut dyn Encoder) -> Result<(), RecordError> {
	loop {
		let count = samples.pop_slice(chunk);
		if count == 0 {
			return Ok(());
		}
		let whole = count - count % CHANNELS;
		encoder.write_block(&chunk[..whole])?;
		if count < chunk.len() {
			return Ok(());
		}
	}
}

#[cfg(test)]
mod tests;
