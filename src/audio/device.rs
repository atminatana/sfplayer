//! Output device enumeration and selection (spec F-03): by index or name substring, falling back to the default.

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SupportedBufferSize};

use super::AudioError;
use crate::cli::DeviceSelector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
	/// Position in the enumeration order; what `--audio-out INDEX` refers to.
	pub index: usize,
	pub name: String,
	pub is_default: bool,
	pub default_sample_rate: u32,
	/// Supported buffer size range in frames when the backend reports one.
	pub buffer_range: Option<(u32, u32)>,
}

pub fn list_output_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
	let host = cpal::default_host();
	let default_name = host.default_output_device().and_then(|device| device_name(&device));
	let devices = host.output_devices().map_err(AudioError::Devices)?;
	let mut infos = Vec::new();
	for (index, device) in devices.enumerate() {
		let name = device_name(&device).unwrap_or_else(|| format!("device {index}"));
		// A single unreadable device (e.g. an ALSA hint whose plugin cannot open the hardware)
		// must not hide the whole list: it is skipped with a warning in the log.
		let config = match device.default_output_config() {
			Ok(config) => config,
			Err(source) => {
				tracing::warn!(device = %name, %source, "skipping audio device, cannot read its configuration");
				continue;
			}
		};
		let buffer_range = match config.buffer_size() {
			SupportedBufferSize::Range { min, max } => Some((*min, *max)),
			SupportedBufferSize::Unknown => None,
		};
		infos.push(AudioDeviceInfo {
			index,
			is_default: default_name.as_deref() == Some(name.as_str()),
			name,
			default_sample_rate: config.sample_rate(),
			buffer_range,
		});
	}
	Ok(infos)
}

pub fn default_output_device() -> Result<Device, AudioError> {
	cpal::default_host().default_output_device().ok_or(AudioError::NoOutputDevice)
}

/// Result of resolving the user's device choice.
pub struct SelectedDevice {
	pub device: Device,
	/// The selector was given but matched nothing, so the default device is used instead (logged as a warning).
	pub requested_missing: bool,
}

/// Picks the output device for `selector`; with no selector or no match, the system default is used (F-04 fallback).
pub fn select_output_device(selector: Option<&DeviceSelector>) -> Result<SelectedDevice, AudioError> {
	let Some(selector) = selector else {
		return Ok(SelectedDevice {
			device: default_output_device()?,
			requested_missing: false,
		});
	};
	let mut devices: Vec<Device> = cpal::default_host().output_devices().map_err(AudioError::Devices)?.collect();
	let names: Vec<String> = devices.iter().map(|device| device_name(device).unwrap_or_default()).collect();
	match selector.find(names.iter().map(String::as_str)) {
		Some(index) => Ok(SelectedDevice {
			device: devices.swap_remove(index),
			requested_missing: false,
		}),
		None => {
			tracing::warn!(?selector, "audio output not found, using the default device");
			Ok(SelectedDevice {
				device: default_output_device()?,
				requested_missing: true,
			})
		}
	}
}

/// Device at `index` in the enumeration order shown by `--list-devices` and the F3 dialog.
pub fn output_device_by_index(index: usize) -> Result<Device, AudioError> {
	cpal::default_host()
		.output_devices()
		.map_err(AudioError::Devices)?
		.nth(index)
		.ok_or(AudioError::NoOutputDevice)
}

pub fn device_name(device: &Device) -> Option<String> {
	device.description().ok().map(|description| description.name().to_owned())
}
