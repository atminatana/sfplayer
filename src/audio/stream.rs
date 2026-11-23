//! Opening the output stream (spec F-04; v1.4, E-01): through cpal — stereo f32, the requested buffer
//! size with fallback to the device default — or, for an output taken exclusively, through the
//! program's own WASAPI stream.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, Stream, StreamConfig};
use ringbuf::traits::Split;
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::device::{Driver, OutputDevice};
use super::render::{Command, RecordSource, record_rings};
use super::{AudioError, AudioShared, BlockRenderer};
use crate::midi::MidiEvent;
use crate::synth::SynthEngine;

const OUTPUT_CHANNELS: u16 = 2;
/// Capacity of the MIDI → audio event ring (ADR 0002: sized with headroom, overflow is counted by the producer).
pub const EVENT_RING_CAPACITY: usize = 1024;
pub const COMMAND_RING_CAPACITY: usize = 256;
/// Seconds of stereo audio the recording ring can hold while the recorder thread catches up.
const RECORD_RING_SECONDS: u32 = 4;
/// Events of a MIDI take the recording ring can hold between two looks of the recorder thread.
const RECORD_EVENT_RING_CAPACITY: usize = 4096;
const RETIRED_RING_CAPACITY: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct StreamParams {
	pub buffer_frames: u32,
	/// `None` keeps the device's default rate.
	pub sample_rate: Option<u32>,
}

/// A running output stream; dropping it stops playback.
pub struct AudioStream {
	/// The stream of the driver the output was opened through: all it is kept for is to stop
	/// playing when it is dropped.
	_playing: Box<dyn Any>,
	/// The output as the list names it: `[Driver]: Device`.
	pub label: String,
	pub sample_rate: u32,
	/// Frames of one callback as the driver reports them; `None` when it does not say.
	pub buffer_frames: Option<u32>,
	/// From a block handed to the driver to its sound, as the driver states it; `None` when it does
	/// not say (a cpal stream).
	pub latency: Option<Duration>,
}

/// The other thread's ends of the rings attached to a stream.
pub struct AudioLinks {
	/// MIDI thread → audio: hardware events.
	pub events: HeapProd<MidiEvent>,
	/// MIDI file player thread → audio: the messages of the file being played.
	pub file_events: HeapProd<MidiEvent>,
	/// UI thread → audio: commands and UI-generated notes.
	pub commands: HeapProd<Command>,
	/// audio → UI: engines replaced via `Command::ReplaceEngine`, to be dropped off the audio thread.
	pub retired: HeapCons<Box<dyn SynthEngine>>,
	/// audio → recorder: the output of an audio take (F-20) or the events of a MIDI take (F-23).
	pub recording: RecordSource,
}

/// A render core for a stream at `sample_rate` and the far ends of its rings. Every attempt to open
/// a stream gets fresh ones: a failed attempt takes its core with it.
fn core_with_links(shared: &Arc<AudioShared>, sample_rate: u32) -> (BlockRenderer, AudioLinks) {
	let (events, events_rx) = HeapRb::<MidiEvent>::new(EVENT_RING_CAPACITY).split();
	let (file_events, file_events_rx) = HeapRb::<MidiEvent>::new(EVENT_RING_CAPACITY).split();
	let (commands, commands_rx) = HeapRb::<Command>::new(COMMAND_RING_CAPACITY).split();
	let (retired_tx, retired) = HeapRb::<Box<dyn SynthEngine>>::new(RETIRED_RING_CAPACITY).split();
	let (recording_tx, recording) = record_rings((sample_rate * 2 * RECORD_RING_SECONDS) as usize, RECORD_EVENT_RING_CAPACITY);
	let renderer = BlockRenderer::new(
		events_rx,
		file_events_rx,
		commands_rx,
		retired_tx,
		recording_tx,
		Arc::clone(shared),
		sample_rate,
	);
	(
		renderer,
		AudioLinks {
			events,
			file_events,
			commands,
			retired,
			recording,
		},
	)
}

/// Opens and starts a silent stream (no engine yet); install one with `Command::ReplaceEngine`.
pub fn open_output_stream(
	output: &OutputDevice,
	params: StreamParams,
	shared: &Arc<AudioShared>,
) -> Result<(AudioStream, AudioLinks), AudioError> {
	let label = output.label();
	let default_config = output.device.default_output_config().map_err(|source| AudioError::DeviceConfig {
		device: label.clone(),
		source,
	})?;
	let sample_rate = params.sample_rate.unwrap_or_else(|| default_config.sample_rate());
	match output.driver {
		Driver::Host | Driver::Asio => open_cpal_stream(output, label, sample_rate, params.buffer_frames, shared),
		Driver::WasapiExclusive => open_exclusive_stream(output, label, sample_rate, params.buffer_frames, shared),
	}
}

/// The requested fixed buffer is tried first; if the backend refuses it, the device default is used (F-04).
fn open_cpal_stream(
	output: &OutputDevice,
	label: String,
	sample_rate: u32,
	buffer_frames: u32,
	shared: &Arc<AudioShared>,
) -> Result<(AudioStream, AudioLinks), AudioError> {
	let attempt = |buffer_size: BufferSize| -> Result<(Stream, AudioLinks), AudioError> {
		let (mut renderer, links) = core_with_links(shared, sample_rate);
		let data_callback = move |output: &mut [f32], _: &cpal::OutputCallbackInfo| renderer.fill(output, OUTPUT_CHANNELS as usize);
		let xrun_counter = Arc::clone(shared);
		let error_callback = move |error: cpal::Error| {
			if error.kind() == ErrorKind::Xrun {
				xrun_counter.record_xrun();
			} else {
				tracing::warn!(%error, "audio stream error");
			}
		};
		let config = StreamConfig {
			channels: OUTPUT_CHANNELS,
			sample_rate,
			buffer_size,
		};
		let stream = output
			.device
			.build_output_stream(config, data_callback, error_callback, None)
			.map_err(|source| AudioError::BuildStream {
				device: label.clone(),
				source,
			})?;
		Ok((stream, links))
	};

	let requested = BufferSize::Fixed(buffer_frames);
	let (stream, links, buffer_size) = match attempt(requested) {
		Ok((stream, links)) => (stream, links, requested),
		Err(AudioError::BuildStream { source, .. }) => {
			tracing::warn!(error = %source, frames = buffer_frames, "fixed buffer size rejected, using device default");
			let (stream, links) = attempt(BufferSize::Default)?;
			(stream, links, BufferSize::Default)
		}
		Err(other) => return Err(other),
	};
	stream.play().map_err(|source| AudioError::Play {
		device: label.clone(),
		source,
	})?;
	// What was asked for is not what a callback gets: a shared WASAPI stream runs at the period of
	// the device whatever the request (spec v1.4 §2).
	let callback_frames = stream.buffer_size().ok();
	tracing::info!(device = %label, sample_rate, requested = ?buffer_size, ?callback_frames, "audio stream started");
	Ok((
		AudioStream {
			_playing: Box::new(stream),
			label,
			sample_rate,
			buffer_frames: callback_frames,
			latency: None,
		},
		links,
	))
}

/// The device is taken for this program alone: its own period, its own integer format (E-01, E-02).
#[cfg(windows)]
fn open_exclusive_stream(
	output: &OutputDevice,
	label: String,
	sample_rate: u32,
	buffer_frames: u32,
	shared: &Arc<AudioShared>,
) -> Result<(AudioStream, AudioLinks), AudioError> {
	use super::exclusive::{self, Request};

	let endpoint = output.device.id().map_err(|source| AudioError::DeviceConfig {
		device: label.clone(),
		source,
	})?;
	let (renderer, links) = core_with_links(shared, sample_rate);
	let request = Request {
		sample_rate,
		buffer_frames,
	};
	let (stream, opened) =
		exclusive::open(endpoint.id(), request, renderer, Arc::clone(shared)).map_err(|reason| AudioError::Exclusive {
			device: label.clone(),
			reason,
		})?;
	tracing::info!(
		device = %label,
		sample_rate,
		format = ?opened.format,
		channels = opened.channels,
		requested = buffer_frames,
		period_frames = opened.period_frames,
		latency_ms = opened.latency.as_secs_f32() * 1000.0,
		"audio stream started"
	);
	Ok((
		AudioStream {
			_playing: Box::new(stream),
			label,
			sample_rate,
			buffer_frames: Some(opened.period_frames),
			latency: Some(opened.latency),
		},
		links,
	))
}

/// There is no exclusive driver off Windows, and no row of the list names one.
#[cfg(not(windows))]
fn open_exclusive_stream(
	_output: &OutputDevice,
	_label: String,
	_sample_rate: u32,
	_buffer_frames: u32,
	_shared: &Arc<AudioShared>,
) -> Result<(AudioStream, AudioLinks), AudioError> {
	Err(AudioError::NoOutputDevice)
}
