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
	#[error("cannot read device configuration for {device}")]
	DeviceConfig {
		device: String,
		#[source]
		source: cpal::Error,
	},
	#[error("cannot open audio stream on {device}")]
	BuildStream {
		device: String,
		#[source]
		source: cpal::Error,
	},
	#[error("cannot start audio stream")]
	Play(#[source] cpal::Error),
	#[error(transparent)]
	Synth(#[from] SynthError),
}
