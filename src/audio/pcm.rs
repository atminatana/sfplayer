//! The f32 render as the integer PCM a device taken exclusively asks for (spec v1.4, E-01).
//! Plain rounding, no dither (the owner's decision, spec v1.4 §8); no allocation, so it may run on
//! the audio thread.

use super::BlockRenderer;

/// Integer formats an exclusive stream can be opened in, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmFormat {
	/// 24 bits in the upper three bytes of four: what the codecs of sound cards take.
	I24In32,
	/// 24 bits in three bytes.
	I24,
	I16,
}

impl PcmFormat {
	/// The order a device is asked in (E-01).
	pub const PREFERRED: [Self; 3] = [Self::I24In32, Self::I24, Self::I16];

	/// Bits of a sample that carry sound.
	pub fn bits(self) -> u16 {
		match self {
			Self::I24In32 | Self::I24 => 24,
			Self::I16 => 16,
		}
	}

	/// Bits a sample takes in the stream; the sound is in the upper ones.
	pub fn container_bits(self) -> u16 {
		match self {
			Self::I24In32 => 32,
			Self::I24 => 24,
			Self::I16 => 16,
		}
	}

	pub fn bytes_per_sample(self) -> usize {
		usize::from(self.container_bits() / 8)
	}

	/// The value of full scale: `1.0` maps one step short of it, `-1.0` exactly onto its negative.
	fn full_scale(self) -> f32 {
		match self {
			Self::I24In32 | Self::I24 => 8_388_608.0,
			Self::I16 => 32_768.0,
		}
	}
}

/// Writes `samples` into `out` as little-endian PCM, as many of them as `out` has room for.
/// What is beyond ±1 saturates; not-a-number becomes silence.
pub fn write_pcm(format: PcmFormat, samples: &[f32], out: &mut [u8]) {
	let scale = format.full_scale();
	// A float-to-integer cast saturates and maps NaN to zero; the 24-bit range is narrower than i32.
	let limit = scale as i32;
	// In a container wider than the sample the sound sits in the upper bits.
	let shift = format.container_bits() - format.bits();
	for (sample, bytes) in samples.iter().zip(out.chunks_exact_mut(format.bytes_per_sample())) {
		let value = ((sample * scale).round() as i32).clamp(-limit, limit - 1) << shift;
		bytes.copy_from_slice(&value.to_le_bytes()[..bytes.len()]);
	}
}

/// A block of the render on its way to an integer device: rendered as f32 into a buffer of its own,
/// then written as PCM. The buffer is allocated once, for the largest block the device asks for.
pub struct PcmBlock {
	format: PcmFormat,
	channels: usize,
	samples: Vec<f32>,
}

impl PcmBlock {
	pub fn new(format: PcmFormat, channels: usize, max_frames: usize) -> Self {
		let channels = channels.max(1);
		Self {
			format,
			channels,
			samples: vec![0.0; max_frames * channels],
		}
	}

	/// Bytes one frame takes in the device buffer.
	pub fn frame_bytes(&self) -> usize {
		self.channels * self.format.bytes_per_sample()
	}

	/// Renders as many frames as `out` holds — no more than the block was made for — and writes
	/// them into it. The meter and the recorder have read the f32 render before this (E-04).
	pub fn render(&mut self, renderer: &mut BlockRenderer, out: &mut [u8]) {
		let frames = (out.len() / self.frame_bytes()).min(self.samples.len() / self.channels);
		let samples = &mut self.samples[..frames * self.channels];
		renderer.fill(samples, self.channels);
		write_pcm(self.format, samples, out);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn i16_of(samples: &[f32]) -> Vec<i16> {
		let mut out = vec![0xAA_u8; samples.len() * 2];
		write_pcm(PcmFormat::I16, samples, &mut out);
		out.as_chunks::<2>().0.iter().map(|bytes| i16::from_le_bytes(*bytes)).collect()
	}

	/// A 24-bit sample read back with its sign.
	fn i24_of(samples: &[f32]) -> Vec<i32> {
		let mut out = vec![0xAA_u8; samples.len() * 3];
		write_pcm(PcmFormat::I24, samples, &mut out);
		out.as_chunks::<3>()
			.0
			.iter()
			.map(|bytes| i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8)
			.collect()
	}

	#[test]
	fn silence_is_zero_and_full_scale_is_the_largest_value_of_the_format() {
		assert_eq!(i16_of(&[0.0, 1.0, -1.0]), [0, i16::MAX, i16::MIN]);
		assert_eq!(i24_of(&[0.0, 1.0, -1.0]), [0, 8_388_607, -8_388_608]);
	}

	#[test]
	fn a_sample_is_rounded_to_the_nearest_step() {
		assert_eq!(i16_of(&[0.5, -0.5, 0.25]), [16_384, -16_384, 8_192]);
		// Less than half a step is silence, more than half is one step.
		assert_eq!(i16_of(&[0.4 / 32_768.0, 0.6 / 32_768.0, -0.6 / 32_768.0]), [0, 1, -1]);
		assert_eq!(i24_of(&[0.5, -0.25]), [4_194_304, -2_097_152]);
	}

	#[test]
	fn what_is_beyond_full_scale_saturates_and_not_a_number_is_silence() {
		assert_eq!(
			i16_of(&[1.5, -1.5, f32::INFINITY, f32::NEG_INFINITY, f32::NAN]),
			[i16::MAX, i16::MIN, i16::MAX, i16::MIN, 0]
		);
		assert_eq!(i24_of(&[2.0, -2.0, f32::NAN]), [8_388_607, -8_388_608, 0]);
	}

	#[test]
	fn a_24_bit_sample_takes_three_bytes_low_byte_first() {
		let mut out = [0_u8; 6];
		// 0x010203 and its negative.
		let value = 66_051.0 / 8_388_608.0;
		write_pcm(PcmFormat::I24, &[value, -value], &mut out);
		assert_eq!(out, [0x03, 0x02, 0x01, 0xFD, 0xFD, 0xFE]);
	}

	#[test]
	fn a_24_bit_sample_in_a_32_bit_container_sits_in_its_upper_three_bytes() {
		let mut out = [0xAA_u8; 16];
		let value = 66_051.0 / 8_388_608.0;
		write_pcm(PcmFormat::I24In32, &[value, -value, 1.0, -1.0], &mut out);
		assert_eq!(
			out,
			[
				0x00, 0x03, 0x02, 0x01, // 0x010203 with an empty low byte
				0x00, 0xFD, 0xFD, 0xFE, // its negative
				0x00, 0xFF, 0xFF, 0x7F, // full scale
				0x00, 0x00, 0x00, 0x80, // negative full scale
			]
		);
	}

	#[test]
	fn only_as_many_samples_are_written_as_the_output_has_room_for() {
		let mut out = [0xAA_u8; 5];
		write_pcm(PcmFormat::I16, &[1.0, 1.0, 1.0], &mut out);
		assert_eq!(out, [0xFF, 0x7F, 0xFF, 0x7F, 0xAA], "the odd byte at the end is left alone");
	}

	#[test]
	fn the_formats_are_asked_for_best_first() {
		assert_eq!(PcmFormat::PREFERRED, [PcmFormat::I24In32, PcmFormat::I24, PcmFormat::I16]);
		let bytes = PcmFormat::PREFERRED.map(PcmFormat::bytes_per_sample);
		assert_eq!(bytes, [4, 3, 2]);
		assert_eq!(PcmFormat::PREFERRED.map(PcmFormat::bits), [24, 24, 16], "the bits that carry sound");
	}
}
