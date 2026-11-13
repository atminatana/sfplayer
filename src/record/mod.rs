//! Recording (spec F-20, F-23; ADR 0005, 0015): the audio callback drops interleaved f32 frames into an
//! SPSC ring — or, for a MIDI take, the events it feeds the engine into another; this module's thread
//! drains the ring of the take and writes FLAC, Ogg Vorbis or a Standard MIDI File. Nothing here runs
//! on the audio thread.
//!
//! The recorder's ends of the rings are lent to it for one take and come back when the take ends —
//! saved, cancelled, or not even started: without them nothing could be recorded until the audio
//! stream was reopened.

mod flac;
mod midi;
mod vorbis;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ringbuf::HeapCons;
use ringbuf::traits::Consumer;

use crate::audio::RecordSource;
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
		RecordFormat::Midi => "mid",
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
	/// Ogg Vorbis quality 0.0..=1.0 (only Ogg Vorbis uses it).
	pub vorbis_quality: f32,
}

/// What a take is written with: an audio encoder fed samples, or a MIDI file fed events.
enum Writer {
	Audio(Box<dyn Encoder>),
	Midi(Box<midi::MidiTake>),
}

impl Writer {
	fn open(format: RecordFormat, path: &Path, params: RecordParams) -> Result<Self, RecordError> {
		Ok(match format {
			RecordFormat::Flac => Self::Audio(Box::new(flac::FlacFile::create(path, params.sample_rate)?)),
			RecordFormat::OggVorbis => Self::Audio(Box::new(vorbis::VorbisFile::create(
				path,
				params.sample_rate,
				params.vorbis_quality,
			)?)),
			RecordFormat::Midi => Self::Midi(Box::new(midi::MidiTake::create(path, params.sample_rate)?)),
		})
	}

	/// Moves what the audio thread has put into the ring of this kind of take to the file.
	fn drain(&mut self, rings: &mut RecordSource, chunk: &mut [f32]) -> Result<(), RecordError> {
		match self {
			Self::Audio(encoder) => drain(&mut rings.samples, chunk, encoder.as_mut()),
			Self::Midi(take) => {
				let mut is_written = false;
				while let Some(event) = rings.events.try_pop() {
					take.write(event)?;
					is_written = true;
				}
				// A MIDI take is a few bytes a second: left in the writer's buffer they would not show
				// in the size of the file until the take ends.
				if is_written { take.flush() } else { Ok(()) }
			}
		}
	}

	fn finish(self) -> Result<(), RecordError> {
		match self {
			Self::Audio(encoder) => encoder.finish(),
			Self::Midi(take) => take.finish(),
		}
	}
}

/// What the recorder thread hands back: the rings and the outcome of writing the file.
type TakeOutcome = (RecordSource, Result<(), RecordError>);

/// How a take is to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
	/// Write what is left in the ring and finalise the file.
	Finish,
	/// Write nothing more; the file is thrown away.
	Cancel,
}

/// A finished take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Take {
	pub path: PathBuf,
	/// Size of the file in bytes.
	pub bytes: u64,
}

/// Why a take did not start, with the rings whenever they could be kept.
pub struct StartError {
	pub error: RecordError,
	pub rings: Option<RecordSource>,
}

impl std::fmt::Debug for StartError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("StartError")
			.field("error", &self.error)
			.field("rings_returned", &self.rings.is_some())
			.finish()
	}
}

/// A recording in progress. `stop` finalises the file, `cancel` throws it away; both hand the rings
/// back for the next take.
pub struct Recorder {
	ending: Option<Sender<Ending>>,
	thread: Option<JoinHandle<Option<TakeOutcome>>>,
	path: PathBuf,
	started: Instant,
	/// Bytes of the file so far, published by the recorder thread.
	bytes: Arc<AtomicU64>,
}

impl Recorder {
	/// Creates the file (on the recorder thread, reporting back before returning), then keeps draining
	/// the ring the format records from.
	pub fn start(format: RecordFormat, path: PathBuf, params: RecordParams, mut rings: RecordSource) -> Result<Self, StartError> {
		// Drop whatever was sitting in the rings before the user pressed record.
		while rings.samples.try_pop().is_some() {}
		while rings.events.try_pop().is_some() {}
		let (ending, ending_signal) = mpsc::channel::<Ending>();
		let (opened_tx, opened_rx) = mpsc::channel::<Result<(), RecordError>>();
		// The rings travel to the thread only once the thread exists: if it cannot be started, they
		// are still here to give back.
		let (rings_tx, rings_rx) = mpsc::channel::<RecordSource>();
		let bytes = Arc::new(AtomicU64::new(0));
		let published = Arc::clone(&bytes);
		let thread_path = path.clone();
		let spawned = std::thread::Builder::new().name("recorder".to_owned()).spawn(move || {
			let mut rings = rings_rx.recv().ok()?;
			let mut writer = match Writer::open(format, &thread_path, params) {
				Ok(writer) => {
					let _ = opened_tx.send(Ok(()));
					writer
				}
				Err(error) => {
					let _ = opened_tx.send(Err(error));
					return Some((rings, Ok(())));
				}
			};
			let mut chunk = vec![0.0f32; DRAIN_CHUNK_SAMPLES];
			let result = loop {
				let ending = match ending_signal.recv_timeout(POLL_INTERVAL) {
					Ok(ending) => Some(ending),
					Err(RecvTimeoutError::Timeout) => None,
					// The recorder was dropped without a word: the take is kept.
					Err(RecvTimeoutError::Disconnected) => Some(Ending::Finish),
				};
				if ending == Some(Ending::Cancel) {
					// The writer is dropped unfinished, which closes the file; the caller removes it.
					break Ok(());
				}
				if let Err(error) = writer.drain(&mut rings, &mut chunk) {
					break Err(error);
				}
				if ending == Some(Ending::Finish) {
					break writer.finish();
				}
				published.store(file_size(&thread_path), Ordering::Relaxed);
			};
			Some((rings, result))
		});
		let thread = match spawned {
			Ok(thread) => thread,
			Err(source) => {
				return Err(StartError {
					error: RecordError::Create { path, source },
					rings: Some(rings),
				});
			}
		};
		if let Err(unsent) = rings_tx.send(rings) {
			return Err(StartError {
				error: RecordError::ThreadPanicked,
				rings: Some(unsent.0),
			});
		}
		let error = match opened_rx.recv() {
			Ok(Ok(())) => {
				tracing::info!(path = %path.display(), ?format, "recording started");
				return Ok(Self {
					ending: Some(ending),
					thread: Some(thread),
					path,
					started: Instant::now(),
					bytes,
				});
			}
			Ok(Err(error)) => error,
			Err(_) => RecordError::ThreadPanicked,
		};
		// The thread has returned; its join brings the rings back. A writer that failed after creating
		// its file leaves the file behind: it holds no take.
		let rings = thread.join().ok().flatten().map(|(rings, _)| rings);
		let _ = std::fs::remove_file(&path);
		Err(StartError { error, rings })
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	pub fn elapsed(&self) -> Duration {
		self.started.elapsed()
	}

	/// Size of the file so far, in bytes; it trails what was played by what the writer still holds.
	pub fn bytes(&self) -> u64 {
		self.bytes.load(Ordering::Relaxed)
	}

	/// Stops, finalises the file and returns the rings plus the take or the error.
	pub fn stop(mut self) -> (Option<RecordSource>, Result<Take, RecordError>) {
		match self.end(Ending::Finish) {
			Some((rings, Ok(()))) => {
				tracing::info!(path = %self.path.display(), "recording saved");
				let take = Take {
					path: self.path.clone(),
					bytes: file_size(&self.path),
				};
				(Some(rings), Ok(take))
			}
			Some((rings, Err(error))) => (Some(rings), Err(error)),
			None => (None, Err(RecordError::ThreadPanicked)),
		}
	}

	/// Stops without keeping the take: the file is removed. Returns the rings and, if the file could
	/// not be removed, why.
	pub fn cancel(mut self) -> (Option<RecordSource>, Result<(), RecordError>) {
		let rings = self.end(Ending::Cancel).map(|(rings, _)| rings);
		let removed = std::fs::remove_file(&self.path).map_err(|source| RecordError::Write {
			path: self.path.clone(),
			source,
		});
		tracing::info!(path = %self.path.display(), "recording cancelled");
		(rings, removed)
	}

	/// Tells the thread how to end and waits for it; `None` if the thread panicked.
	fn end(&mut self, ending: Ending) -> Option<TakeOutcome> {
		if let Some(signal) = self.ending.take() {
			let _ = signal.send(ending);
		}
		self.thread.take()?.join().ok().flatten()
	}
}

/// Size of the file at `path`; 0 when it cannot be asked.
fn file_size(path: &Path) -> u64 {
	std::fs::metadata(path).map_or(0, |metadata| metadata.len())
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
