//! Opening the cpal output stream (spec F-04): stereo in the device's sample format (f32 render converted per
//! sample, ADR 0012), requested buffer size with fallback to the device default.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{BufferSize, Device, ErrorKind, FromSample, I24, SampleFormat, SizedSample, Stream, StreamConfig};
use ringbuf::traits::Split;
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::device::OutputDevice;
use super::render::Command;
use super::{AudioError, AudioShared, BlockRenderer};
use crate::midi::MidiEvent;
use crate::synth::SynthEngine;

const OUTPUT_CHANNELS: u16 = 2;
/// Capacity of the MIDI → audio event ring (ADR 0002: sized with headroom, overflow is counted by the producer).
pub const EVENT_RING_CAPACITY: usize = 1024;
pub const COMMAND_RING_CAPACITY: usize = 256;
/// Seconds of stereo audio the recording ring can hold while the recorder thread catches up.
const RECORD_RING_SECONDS: u32 = 4;
const RETIRED_RING_CAPACITY: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct StreamParams {
	pub buffer_frames: u32,
	/// `None` keeps the device's default rate.
	pub sample_rate: Option<u32>,
}

/// A running output stream; dropping it stops playback.
pub struct AudioStream {
	_stream: Stream,
	pub device_name: String,
	pub sample_rate: u32,
	pub buffer_size: BufferSize,
}

/// The other thread's ends of the rings attached to a stream.
pub struct AudioLinks {
	/// MIDI thread → audio: hardware events.
	pub events: HeapProd<MidiEvent>,
	/// UI thread → audio: commands and UI-generated notes.
	pub commands: HeapProd<Command>,
	/// audio → UI: engines replaced via `Command::ReplaceEngine`, to be dropped off the audio thread.
	pub retired: HeapCons<Box<dyn SynthEngine>>,
	/// audio → recorder: interleaved stereo f32 while recording (F-20).
	pub recording: HeapCons<f32>,
}

/// Opens and starts a silent stream (no engine yet); install one with `Command::ReplaceEngine`.
///
/// The requested fixed buffer is tried first; if the backend refuses it, the device default is used (F-04).
/// Each attempt owns fresh rings because cpal consumes the callbacks even when it fails.
pub fn open_output_stream(
	device: &OutputDevice,
	params: StreamParams,
	shared: &Arc<AudioShared>,
) -> Result<(AudioStream, AudioLinks), AudioError> {
	let device_name = device.name.clone();
	let default_config = device.device.default_output_config().map_err(|source| AudioError::DeviceConfig {
		device: device_name.clone(),
		source,
	})?;
	let sample_rate = params.sample_rate.unwrap_or_else(|| default_config.sample_rate());
	// ASIO drivers speak their own format (mostly Int32) and cpal does not convert; WASAPI shared is f32.
	let sample_format = default_config.sample_format();

	let attempt = |buffer_size: BufferSize| -> Result<(Stream, AudioLinks), AudioError> {
		let (events, events_rx) = HeapRb::<MidiEvent>::new(EVENT_RING_CAPACITY).split();
		let (commands, commands_rx) = HeapRb::<Command>::new(COMMAND_RING_CAPACITY).split();
		let (retired_tx, retired) = HeapRb::<Box<dyn SynthEngine>>::new(RETIRED_RING_CAPACITY).split();
		let (recording_tx, recording) = HeapRb::<f32>::new((sample_rate * 2 * RECORD_RING_SECONDS) as usize).split();
		let renderer = BlockRenderer::new(events_rx, commands_rx, retired_tx, recording_tx, Arc::clone(shared), sample_rate);
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
		let stream = match sample_format {
			SampleFormat::F32 => build_stream::<f32>(&device.device, config, renderer, error_callback),
			SampleFormat::I16 => build_stream::<i16>(&device.device, config, renderer, error_callback),
			SampleFormat::I32 => build_stream::<i32>(&device.device, config, renderer, error_callback),
			SampleFormat::I24 => build_stream::<I24>(&device.device, config, renderer, error_callback),
			SampleFormat::F64 => build_stream::<f64>(&device.device, config, renderer, error_callback),
			other => Err(cpal::Error::with_message(
				ErrorKind::UnsupportedConfig,
				format!("sample format {other} is not supported"),
			)),
		}
		.map_err(|source| AudioError::BuildStream {
			device: device_name.clone(),
			source,
		})?;
		Ok((
			stream,
			AudioLinks {
				events,
				commands,
				retired,
				recording,
			},
		))
	};

	let requested = BufferSize::Fixed(params.buffer_frames);
	let (stream, links, buffer_size) = match attempt(requested) {
		Ok((stream, links)) => (stream, links, requested),
		Err(AudioError::BuildStream { source, .. }) => {
			tracing::warn!(error = %source, frames = params.buffer_frames, "fixed buffer size rejected, using device default");
			let (stream, links) = attempt(BufferSize::Default)?;
			(stream, links, BufferSize::Default)
		}
		Err(other) => return Err(other),
	};
	stream.play().map_err(|source| AudioError::Play {
		device: device_name.clone(),
		source,
	})?;
	tracing::info!(
		device = %device_name,
		backend = device.backend.label(),
		%sample_format,
		sample_rate,
		?buffer_size,
		"audio stream started"
	);
	Ok((
		AudioStream {
			_stream: stream,
			device_name,
			sample_rate,
			buffer_size,
		},
		links,
	))
}

/// Builds the stream for the device's own sample type; the callback converts from the f32 render (ADR 0012).
fn build_stream<T: SizedSample + FromSample<f32>>(
	device: &Device,
	config: StreamConfig,
	mut renderer: BlockRenderer,
	error_callback: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, cpal::Error> {
	device.build_output_stream(
		config,
		move |output: &mut [T], _: &cpal::OutputCallbackInfo| renderer.fill(output, OUTPUT_CHANNELS as usize),
		error_callback,
		None,
	)
}
