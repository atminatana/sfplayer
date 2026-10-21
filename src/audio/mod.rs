//! Audio output: device enumeration, the cpal stream and the real-time render core.
//! Threading model and lock-free exchange: ADR 0002; Windows backend choice: ADR 0003.

mod device;
pub mod meter;
pub mod mixer;
mod render;
mod shared;
mod stream;

pub use device::{
	AudioDeviceInfo, SelectedDevice, default_output_device, list_output_devices, output_device_by_index, select_output_device,
};
pub use meter::Levels;
pub use mixer::Mixer;
pub use render::{BlockRenderer, Command};
pub use shared::AudioShared;
pub use stream::{AudioLinks, AudioStream, StreamParams, open_output_stream};

use crate::synth::SynthError;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
	#[error("no audio output device available")]
	NoOutputDevice,
	#[error("cannot enumerate audio devices")]
	Devices(#[source] cpal::Error),
	#[error("cannot read device configuration for {device}: {source}")]
	DeviceConfig {
		device: String,
		#[source]
		source: cpal::Error,
	},
	#[error("cannot open audio stream on {device}: {source}")]
	BuildStream {
		device: String,
		#[source]
		source: cpal::Error,
	},
	#[error("cannot start audio stream on {device}: {source}")]
	Play {
		device: String,
		#[source]
		source: cpal::Error,
	},
	#[error(transparent)]
	Synth(#[from] SynthError),
}

#[cfg(test)]
mod tests {
	use super::AudioError;
	use cpal::ErrorKind;

	#[test]
	fn device_config_error_names_the_device_and_the_cause() {
		let error = AudioError::DeviceConfig {
			device: "HDA Intel PCH, ALC897 Analog".to_owned(),
			source: cpal::Error::with_message(ErrorKind::UnsupportedOperation, "Device does not support output"),
		};
		let text = error.to_string();
		assert!(
			text.contains("HDA Intel PCH, ALC897 Analog") && text.contains("Device does not support output"),
			"{text}"
		);
	}

	#[test]
	fn build_stream_error_names_the_device_and_the_cause() {
		let error = AudioError::BuildStream {
			device: "HDA ATI HDMI, Acer VG270K".to_owned(),
			source: cpal::Error::with_message(ErrorKind::BackendError, "The requested device is temporarily busy."),
		};
		let text = error.to_string();
		assert!(
			text.contains("HDA ATI HDMI, Acer VG270K") && text.contains("temporarily busy"),
			"{text}"
		);
	}

	#[test]
	fn play_error_names_the_device_and_the_cause() {
		let error = AudioError::Play {
			device: "Default Audio Device".to_owned(),
			source: cpal::Error::with_message(ErrorKind::BackendError, "Device disconnected"),
		};
		let text = error.to_string();
		assert!(
			text.contains("Default Audio Device") && text.contains("Device disconnected"),
			"{text}"
		);
	}
}
