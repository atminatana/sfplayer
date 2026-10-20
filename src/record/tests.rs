//! Encoder round trips (FLAC via claxon, Ogg Vorbis via lewton) and the recorder thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::TimeZone;
use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};

use super::*;

static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);
const SAMPLE_RATE: u32 = 48_000;

fn temp_path(extension: &str) -> PathBuf {
	let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
	std::env::temp_dir().join(format!("sf2_player-record-{}-{unique}.{extension}", std::process::id()))
}

/// One second of a 440 Hz sine at amplitude 0.5 on the left, 0.25 on the right, interleaved.
fn stereo_sine(frames: usize) -> Vec<f32> {
	(0..frames)
		.flat_map(|i| {
			let phase = i as f32 / SAMPLE_RATE as f32 * 440.0 * std::f32::consts::TAU;
			[0.5 * phase.sin(), 0.25 * phase.sin()]
		})
		.collect()
}

fn rms(samples: impl Iterator<Item = f32>) -> f32 {
	let (sum, count) = samples.fold((0.0f64, 0usize), |(sum, count), sample| {
		(sum + f64::from(sample * sample), count + 1)
	});
	if count == 0 { 0.0 } else { (sum / count as f64).sqrt() as f32 }
}

#[test]
fn file_names_carry_date_time_and_extension() {
	let at = chrono::Local.with_ymd_and_hms(2026, 10, 20, 12, 3, 41).unwrap();
	assert_eq!(
		file_name(Path::new("rec"), RecordFormat::Flac, at),
		PathBuf::from("rec").join("2026-10-20_12-03-41.flac")
	);
	assert_eq!(
		file_name(Path::new("rec"), RecordFormat::OggVorbis, at),
		PathBuf::from("rec").join("2026-10-20_12-03-41.ogg")
	);
}

#[test]
fn flac_round_trip_is_lossless_to_24_bits() {
	let path = temp_path("flac");
	let input = stereo_sine(SAMPLE_RATE as usize + 1234);
	let mut encoder: Box<dyn Encoder> = Box::new(flac::FlacFile::create(&path, SAMPLE_RATE).expect("create"));
	for block in input.chunks(1000) {
		encoder.write_block(block).expect("write");
	}
	encoder.finish().expect("finish");

	let bytes = std::fs::read(&path).expect("read back");
	assert_eq!(&bytes[..4], b"fLaC");
	let mut reader = claxon::FlacReader::open(&path).expect("claxon opens");
	let info = reader.streaminfo();
	assert_eq!((info.sample_rate, info.channels, info.bits_per_sample), (SAMPLE_RATE, 2, 24));
	assert_eq!(
		info.samples,
		Some((SAMPLE_RATE as usize + 1234) as u64),
		"total samples rewritten in the header"
	);
	let decoded: Vec<i32> = reader.samples().map(|sample| sample.expect("sample")).collect();
	assert_eq!(decoded.len(), input.len());
	let scale = 8_388_607.0f32;
	let max_error = decoded
		.iter()
		.zip(&input)
		.map(|(d, i)| ((*d as f32 / scale) - i).abs())
		.fold(0.0f32, f32::max);
	assert!(max_error < 2.0 / scale, "24-bit quantisation only: {max_error}");
	std::fs::remove_file(&path).ok();
}

#[test]
fn vorbis_round_trip_keeps_the_signal_level() {
	let path = temp_path("ogg");
	let frames = SAMPLE_RATE as usize;
	let input = stereo_sine(frames);
	let mut encoder: Box<dyn Encoder> = Box::new(vorbis::VorbisFile::create(&path, SAMPLE_RATE, 0.5).expect("create"));
	for block in input.chunks(2048) {
		encoder.write_block(block).expect("write");
	}
	encoder.finish().expect("finish");

	let bytes = std::fs::read(&path).expect("read back");
	assert_eq!(&bytes[..4], b"OggS");
	let file = std::fs::File::open(&path).expect("open");
	let mut reader = lewton::inside_ogg::OggStreamReader::new(file).expect("lewton opens");
	assert_eq!(reader.ident_hdr.audio_sample_rate, SAMPLE_RATE);
	assert_eq!(reader.ident_hdr.audio_channels, 2);
	let mut left = Vec::new();
	while let Some(packet) = reader.read_dec_packet_itl().expect("decode") {
		left.extend(packet.iter().step_by(2).map(|sample| f32::from(*sample) / 32768.0));
	}
	assert!(
		(left.len() as f32 - frames as f32).abs() < frames as f32 * 0.05,
		"decoded {} frames",
		left.len()
	);
	let level = rms(left.iter().copied());
	assert!((level - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.03, "left rms {level}");
	std::fs::remove_file(&path).ok();
}

#[test]
fn recorder_thread_drains_the_ring_and_returns_the_consumer() {
	let path = temp_path("flac");
	let (mut producer, consumer) = HeapRb::<f32>::new(SAMPLE_RATE as usize * 4).split();
	producer.push_slice(&[9.0; 7]); // stale samples before record is pressed must be discarded
	let params = RecordParams {
		sample_rate: SAMPLE_RATE,
		vorbis_quality: 0.5,
	};
	let recorder = Recorder::start(RecordFormat::Flac, path.clone(), params, consumer).expect("start");
	let input = stereo_sine(SAMPLE_RATE as usize / 2);
	for block in input.chunks(4096) {
		assert_eq!(producer.push_slice(block), block.len());
		std::thread::sleep(Duration::from_millis(5));
	}
	let (consumer, result) = recorder.stop();
	assert!(consumer.is_some(), "ring consumer comes back for the next take");
	assert_eq!(result.expect("saved"), path);
	let mut reader = claxon::FlacReader::open(&path).expect("valid FLAC");
	assert_eq!(
		reader.streaminfo().samples,
		Some((SAMPLE_RATE as usize / 2) as u64),
		"no stale samples, nothing lost"
	);
	let decoded = reader.samples().count();
	assert_eq!(decoded, input.len());
	std::fs::remove_file(&path).ok();
}

#[test]
fn unwritable_path_is_a_create_error() {
	let path = PathBuf::from("definitely/not/here/take.flac");
	let (_, consumer) = HeapRb::<f32>::new(16).split();
	let params = RecordParams {
		sample_rate: SAMPLE_RATE,
		vorbis_quality: 0.5,
	};
	assert!(matches!(
		Recorder::start(RecordFormat::Flac, path, params, consumer),
		Err(RecordError::Create { .. })
	));
}
