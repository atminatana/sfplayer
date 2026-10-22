//! Output device enumeration and selection (spec F-03): by index or name substring, falling back to the default.
//!
//! One list for every backend: the default host's outputs first, then — in a Windows build with the
//! `asio` feature — the ASIO drivers, tagged `[ASIO]` in their names (ADR 0012).

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SupportedBufferSize};

use super::AudioError;
use crate::cli::DeviceSelector;

/// Appended to the name of every ASIO device, so lists, the F3 line and `audio.output` tell the backends apart.
const ASIO_TAG: &str = " [ASIO]";

/// Which cpal host a device belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
	/// The OS default host: WASAPI on Windows, CoreAudio on macOS, ALSA/PipeWire on Linux.
	#[default]
	Default,
	/// Steinberg ASIO (Windows, feature `asio`).
	Asio,
}

impl Backend {
	pub fn label(self) -> &'static str {
		match self {
			Self::Default => "default",
			Self::Asio => "asio",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
	/// Position in the enumeration order; what `--audio-out INDEX` refers to.
	pub index: usize,
	/// Display name; ASIO devices carry the `[ASIO]` tag.
	pub name: String,
	pub backend: Backend,
	pub is_default: bool,
	pub default_sample_rate: u32,
	/// Supported buffer size range in frames when the backend reports one.
	pub buffer_range: Option<(u32, u32)>,
}

/// An output device together with its backend and display name.
pub struct OutputDevice {
	pub device: Device,
	pub backend: Backend,
	pub name: String,
}

pub fn list_output_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
	let default_name = cpal::default_host().default_output_device().and_then(|device| device_name(&device));
	let mut infos = Vec::new();
	for (index, output) in enumerate_outputs()?.into_iter().enumerate() {
		// A single unreadable device (e.g. an ALSA hint whose plugin cannot open the hardware)
		// must not hide the whole list: it is skipped with a warning in the log.
		let config = match output.device.default_output_config() {
			Ok(config) => config,
			Err(source) => {
				tracing::warn!(device = %output.name, %source, "skipping audio device, cannot read its configuration");
				continue;
			}
		};
		let buffer_range = match config.buffer_size() {
			SupportedBufferSize::Range { min, max } => Some((*min, *max)),
			SupportedBufferSize::Unknown => None,
		};
		infos.push(AudioDeviceInfo {
			index,
			is_default: is_default_output(output.backend, &output.name, default_name.as_deref()),
			name: output.name,
			backend: output.backend,
			default_sample_rate: config.sample_rate(),
			buffer_range,
		});
	}
	Ok(infos)
}

pub fn default_output_device() -> Result<OutputDevice, AudioError> {
	let device = cpal::default_host().default_output_device().ok_or(AudioError::NoOutputDevice)?;
	let name = device_name(&device).unwrap_or_else(|| "unknown".to_owned());
	Ok(OutputDevice {
		device,
		backend: Backend::Default,
		name,
	})
}

/// Result of resolving the user's device choice.
pub struct SelectedDevice {
	pub device: OutputDevice,
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
	let mut outputs = enumerate_outputs()?;
	match selector.find(outputs.iter().map(|output| output.name.as_str())) {
		Some(index) => Ok(SelectedDevice {
			device: outputs.swap_remove(index),
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
pub fn output_device_by_index(index: usize) -> Result<OutputDevice, AudioError> {
	enumerate_outputs()?.into_iter().nth(index).ok_or(AudioError::NoOutputDevice)
}

pub fn device_name(device: &Device) -> Option<String> {
	device.description().ok().map(|description| description.name().to_owned())
}

/// Every output in list order: the default host first, so indices from before the ASIO feature stay valid.
fn enumerate_outputs() -> Result<Vec<OutputDevice>, AudioError> {
	let default_devices: Vec<(Option<String>, Device)> = cpal::default_host()
		.output_devices()
		.map_err(AudioError::Devices)?
		.map(|device| (device_name(&device), device))
		.collect();
	let asio_devices: Vec<(Option<String>, Device)> = asio_output_devices()
		.into_iter()
		.map(|device| (device_name(&device), device))
		.collect();
	Ok(label_outputs(default_devices, asio_devices)
		.into_iter()
		.map(|(name, backend, device)| OutputDevice { device, backend, name })
		.collect())
}

/// The pure part of the enumeration: default-host entries keep their position and raw names, ASIO
/// entries follow with the tag; a device without a readable name gets a positional one.
fn label_outputs<D>(default: Vec<(Option<String>, D)>, asio: Vec<(Option<String>, D)>) -> Vec<(String, Backend, D)> {
	let default = default
		.into_iter()
		.enumerate()
		.map(|(position, (name, device))| (name.unwrap_or_else(|| format!("device {position}")), Backend::Default, device));
	let asio = asio.into_iter().enumerate().map(|(position, (name, device))| {
		let name = name.unwrap_or_else(|| format!("asio device {position}"));
		(format!("{name}{ASIO_TAG}"), Backend::Asio, device)
	});
	default.chain(asio).collect()
}

/// The `*` marker belongs to the default host's default device only: ASIO has no notion of one.
fn is_default_output(backend: Backend, name: &str, default_name: Option<&str>) -> bool {
	backend == Backend::Default && default_name == Some(name)
}

#[cfg(all(windows, feature = "asio"))]
fn asio_output_devices() -> Vec<Device> {
	let host = match cpal::host_from_id(cpal::HostId::Asio) {
		Ok(host) => host,
		Err(error) => {
			tracing::warn!(%error, "ASIO host unavailable, listing the default host only");
			return Vec::new();
		}
	};
	match host.output_devices() {
		Ok(devices) => devices.collect(),
		Err(error) => {
			tracing::warn!(%error, "cannot enumerate ASIO devices, listing the default host only");
			Vec::new()
		}
	}
}

#[cfg(not(all(windows, feature = "asio")))]
fn asio_output_devices() -> Vec<Device> {
	Vec::new()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn labelled() -> Vec<(String, Backend, char)> {
		label_outputs(
			vec![(Some("Speakers".to_owned()), 'a'), (None, 'b')],
			vec![(Some("Realtek ASIO".to_owned()), 'c'), (None, 'd')],
		)
	}

	#[test]
	fn default_host_devices_keep_their_order_and_asio_ones_follow_tagged() {
		assert_eq!(
			labelled(),
			vec![
				("Speakers".to_owned(), Backend::Default, 'a'),
				("device 1".to_owned(), Backend::Default, 'b'),
				("Realtek ASIO [ASIO]".to_owned(), Backend::Asio, 'c'),
				("asio device 1 [ASIO]".to_owned(), Backend::Asio, 'd'),
			]
		);
	}

	#[test]
	fn selector_reaches_asio_devices_by_tag_name_or_index() {
		let labelled = labelled();
		let names = || labelled.iter().map(|(name, _, _)| name.as_str());
		assert_eq!(
			DeviceSelector::Name("[asio]".into()).find(names()),
			Some(2),
			"tag is part of the name"
		);
		assert_eq!(DeviceSelector::Name("realtek".into()).find(names()), Some(2));
		assert_eq!(DeviceSelector::Index(3).find(names()), Some(3));
		assert_eq!(DeviceSelector::Name("Speakers".into()).find(names()), Some(0));
	}

	#[test]
	fn only_the_default_host_default_device_is_marked() {
		assert!(is_default_output(Backend::Default, "Speakers", Some("Speakers")));
		assert!(!is_default_output(Backend::Default, "HDMI", Some("Speakers")));
		assert!(!is_default_output(Backend::Asio, "Speakers", Some("Speakers")));
		assert!(!is_default_output(Backend::Default, "Speakers", None));
	}

	#[test]
	fn backend_labels_are_stable_log_tokens() {
		assert_eq!(Backend::Default.label(), "default");
		assert_eq!(Backend::Asio.label(), "asio");
		assert_eq!(Backend::default(), Backend::Default);
	}
}
