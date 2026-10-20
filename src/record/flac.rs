//! Streaming FLAC writer on top of `flacenc` (pure Rust): 24-bit, frames written as they complete, STREAMINFO
//! rewritten with the final sample count when the recording stops.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Stream};
use flacenc::config::Encoder as FlacConfig;
use flacenc::error::Verified;
use flacenc::error::Verify;
use flacenc::source::{Fill, FrameBuf};

use super::{CHANNELS, Encoder, RecordError};

const BITS_PER_SAMPLE: usize = 24;
const FULL_SCALE: f32 = 8_388_607.0;

pub struct FlacFile {
	path: PathBuf,
	file: BufWriter<File>,
	config: Verified<FlacConfig>,
	stream: Stream,
	block_size: usize,
	/// Interleaved i32 samples waiting for a full block.
	pending: Vec<i32>,
	frame_number: usize,
	total_frames: usize,
}

impl FlacFile {
	pub fn create(path: &Path, sample_rate: u32) -> Result<Self, RecordError> {
		let file = File::create(path).map_err(|source| RecordError::Create {
			path: path.to_owned(),
			source,
		})?;
		let config = FlacConfig::default()
			.into_verified()
			.map_err(|(_, error)| RecordError::Flac(error.to_string()))?;
		let block_size = config.block_size;
		let stream = Stream::new(sample_rate as usize, CHANNELS, BITS_PER_SAMPLE).map_err(|error| RecordError::Flac(error.to_string()))?;
		let mut flac = Self {
			path: path.to_owned(),
			file: BufWriter::new(file),
			config,
			stream,
			block_size,
			pending: Vec::with_capacity(block_size * CHANNELS),
			frame_number: 0,
			total_frames: 0,
		};
		flac.write_header()?;
		Ok(flac)
	}

	/// "fLaC" + STREAMINFO; written at creation with unknown totals and again at the end with the real count.
	fn write_header(&mut self) -> Result<(), RecordError> {
		let mut sink = ByteSink::new();
		self.stream.write(&mut sink).map_err(|error| RecordError::Flac(error.to_string()))?;
		self.file.write_all(sink.as_slice()).map_err(|source| self.write_error(source))
	}

	fn write_error(&self, source: std::io::Error) -> RecordError {
		RecordError::Write {
			path: self.path.clone(),
			source,
		}
	}

	fn encode_pending(&mut self, frames: usize) -> Result<(), RecordError> {
		let mut buffer = FrameBuf::with_size(CHANNELS, frames).map_err(|error| RecordError::Flac(error.to_string()))?;
		buffer
			.fill_interleaved(&self.pending[..frames * CHANNELS])
			.map_err(|error| RecordError::Flac(error.to_string()))?;
		let frame = flacenc::encode_fixed_size_frame(&self.config, &buffer, self.frame_number, self.stream.stream_info())
			.map_err(|error| RecordError::Flac(error.to_string()))?;
		let mut sink = ByteSink::new();
		frame.write(&mut sink).map_err(|error| RecordError::Flac(error.to_string()))?;
		self.file.write_all(sink.as_slice()).map_err(|source| self.write_error(source))?;
		self.stream.stream_info_mut().update_frame_info(&frame);
		self.pending.drain(..frames * CHANNELS);
		self.frame_number += 1;
		self.total_frames += frames;
		Ok(())
	}
}

impl Encoder for FlacFile {
	fn write_block(&mut self, interleaved: &[f32]) -> Result<(), RecordError> {
		self.pending
			.extend(interleaved.iter().map(|sample| (sample.clamp(-1.0, 1.0) * FULL_SCALE) as i32));
		while self.pending.len() >= self.block_size * CHANNELS {
			self.encode_pending(self.block_size)?;
		}
		Ok(())
	}

	fn finish(mut self: Box<Self>) -> Result<(), RecordError> {
		let remaining = self.pending.len() / CHANNELS;
		if remaining > 0 {
			self.encode_pending(remaining)?;
		}
		self.stream.stream_info_mut().set_total_samples(self.total_frames);
		self.file.flush().map_err(|source| self.write_error(source))?;
		self.file.seek(SeekFrom::Start(0)).map_err(|source| self.write_error(source))?;
		self.write_header()?;
		self.file.flush().map_err(|source| self.write_error(source))
	}
}
