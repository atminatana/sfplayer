//! Opening the cpal output stream (spec F-04): stereo f32, requested buffer size with fallback to the device default.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{BufferSize, Device, ErrorKind, Stream, StreamConfig};
use ringbuf::traits::Split;
use ringbuf::{HeapProd, HeapRb};

use super::device::device_name;
use super::{AudioError, AudioShared, BlockRenderer};
use crate::midi::MidiEvent;
use crate::synth::{SynthEngine, SynthError};

const OUTPUT_CHANNELS: u16 = 2;
/// Capacity of the MIDI → audio event ring (ADR 0002: sized with headroom, overflow is counted by the producer).
pub const EVENT_RING_CAPACITY: usize = 1024;

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

/// Opens and starts the stream, returning it with the producer side of the MIDI event ring.
///
/// The engine is built for the negotiated sample rate. The requested fixed buffer is tried first;
/// if the backend refuses it, the device default is used (F-04). Each attempt owns a fresh engine
/// and ring because cpal consumes the callbacks even when it fails.
pub fn open_output_stream(
	device: &Device,
	params: StreamParams,
	make_engine: &dyn Fn(u32) -> Result<Box<dyn SynthEngine>, SynthError>,
	shared: &Arc<AudioShared>,
) -> Result<(AudioStream, HeapProd<MidiEvent>), AudioError> {
	let device_name = device_name(device).unwrap_or_else(|| "unknown".to_owned());
	let default_config = device.default_output_config().map_err(|source| AudioError::DeviceConfig {
		device: device_name.clone(),
		source,
	})?;
	let sample_rate = params.sample_rate.unwrap_or_else(|| default_config.sample_rate());

	let attempt = |buffer_size: BufferSize| -> Result<(Stream, HeapProd<MidiEvent>), AudioError> {
		let engine = make_engine(sample_rate)?;
		let (producer, consumer) = HeapRb::<MidiEvent>::new(EVENT_RING_CAPACITY).split();
		let mut renderer = BlockRenderer::new(engine, consumer, Arc::clone(shared));
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
		let stream = device
			.build_output_stream(config, data_callback, error_callback, None)
			.map_err(|source| AudioError::BuildStream {
				device: device_name.clone(),
				source,
			})?;
		Ok((stream, producer))
	};

	let requested = BufferSize::Fixed(params.buffer_frames);
	let (stream, producer, buffer_size) = match attempt(requested) {
		Ok((stream, producer)) => (stream, producer, requested),
		Err(AudioError::BuildStream { source, .. }) => {
			tracing::warn!(error = %source, frames = params.buffer_frames, "fixed buffer size rejected, using device default");
			let (stream, producer) = attempt(BufferSize::Default)?;
			(stream, producer, BufferSize::Default)
		}
		Err(other) => return Err(other),
	};
	stream.play().map_err(AudioError::Play)?;
	tracing::info!(device = %device_name, sample_rate, ?buffer_size, "audio stream started");
	Ok((
		AudioStream {
			_stream: stream,
			device_name,
			sample_rate,
			buffer_size,
		},
		producer,
	))
}
