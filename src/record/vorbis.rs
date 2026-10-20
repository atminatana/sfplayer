//! Ogg Vorbis writer on top of `vorbis_rs` (libvorbis built from source via `cc`, see ADR 0005).

use std::fs::File;
use std::io::BufWriter;
use std::num::{NonZeroU8, NonZeroU32};
use std::path::Path;

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoder, VorbisEncoderBuilder};

use super::{CHANNELS, Encoder, RecordError};

pub struct VorbisFile {
	encoder: VorbisEncoder<BufWriter<File>>,
	left: Vec<f32>,
	right: Vec<f32>,
}

impl VorbisFile {
	pub fn create(path: &Path, sample_rate: u32, quality: f32) -> Result<Self, RecordError> {
		let file = File::create(path).map_err(|source| RecordError::Create {
			path: path.to_owned(),
			source,
		})?;
		let rate = NonZeroU32::new(sample_rate).ok_or_else(|| RecordError::Vorbis("sample rate is zero".to_owned()))?;
		let channels = NonZeroU8::new(CHANNELS as u8).expect("two channels");
		let encoder = VorbisEncoderBuilder::new(rate, channels, BufWriter::new(file))
			.map_err(|error| RecordError::Vorbis(error.to_string()))?
			.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
				target_quality: quality.clamp(0.0, 1.0),
			})
			.build()
			.map_err(|error| RecordError::Vorbis(error.to_string()))?;
		Ok(Self {
			encoder,
			left: Vec::new(),
			right: Vec::new(),
		})
	}
}

impl Encoder for VorbisFile {
	fn write_block(&mut self, interleaved: &[f32]) -> Result<(), RecordError> {
		if interleaved.is_empty() {
			return Ok(());
		}
		self.left.clear();
		self.right.clear();
		for [left, right] in interleaved.as_chunks::<CHANNELS>().0 {
			self.left.push(*left);
			self.right.push(*right);
		}
		self.encoder
			.encode_audio_block([&self.left, &self.right])
			.map_err(|error| RecordError::Vorbis(error.to_string()))
	}

	fn finish(self: Box<Self>) -> Result<(), RecordError> {
		self.encoder
			.finish()
			.map(|_| ())
			.map_err(|error| RecordError::Vorbis(error.to_string()))
	}
}
