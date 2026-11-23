//! Output enumeration and selection (spec F-03; v1.4, D-01…D-05): one list for every driver, a row
//! is named `[Driver]: Device`; chosen by index or name substring, falling back to the default.

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SupportedBufferSize};

use super::AudioError;
use crate::cli::DeviceSelector;

/// What the list calls the OS default host of cpal.
#[cfg(target_os = "windows")]
const HOST_DRIVER_NAME: &str = "WASAPI";
#[cfg(target_os = "macos")]
const HOST_DRIVER_NAME: &str = "CoreAudio";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const HOST_DRIVER_NAME: &str = "ALSA";

/// The way sound reaches a device (ADR 0019). The order of the variants is the order of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
	/// The OS default host of cpal: WASAPI in shared mode on Windows, ALSA on Linux.
	Host,
	/// WASAPI with the device taken for this program alone (Windows).
	WasapiExclusive,
	/// Steinberg ASIO (Windows, feature `asio`).
	Asio,
}

impl Driver {
	/// A proper name, the same in every locale (spec v1.4, D-07).
	pub fn name(self) -> &'static str {
		match self {
			Self::Host => HOST_DRIVER_NAME,
			Self::WasapiExclusive => "WASAPI Exclusive",
			Self::Asio => "ASIO",
		}
	}
}

/// The name of an output wherever it is shown or stored: the list, the output row, the log,
/// `--list-devices` and `audio.output`.
pub fn output_label(driver: Driver, name: &str) -> String {
	format!("[{}]: {name}", driver.name())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
	/// Position in the enumeration order; what `--audio-out INDEX` refers to.
	pub index: usize,
	pub driver: Driver,
	/// The device's own name, without the driver.
	pub name: String,
	pub is_default: bool,
	pub default_sample_rate: u32,
	/// Supported buffer size range in frames when the backend reports one.
	pub buffer_range: Option<(u32, u32)>,
}

impl AudioDeviceInfo {
	pub fn label(&self) -> String {
		output_label(self.driver, &self.name)
	}
}

/// An output a stream can be opened on: a device and the driver that plays through it.
pub struct OutputDevice {
	pub driver: Driver,
	/// The device's own name, without the driver.
	pub name: String,
	pub device: Device,
}

impl OutputDevice {
	pub fn label(&self) -> String {
		output_label(self.driver, &self.name)
	}
}

/// Every output of every driver, in the order all indices refer to (D-02, D-04): the devices of the
/// host driver, then — on Windows — the same devices taken exclusively.
fn enumerate_outputs() -> Result<Vec<OutputDevice>, AudioError> {
	let devices = cpal::default_host().output_devices().map_err(AudioError::Devices)?;
	let mut outputs: Vec<OutputDevice> = devices
		.enumerate()
		.map(|(position, device)| OutputDevice {
			driver: Driver::Host,
			name: device_name(&device).unwrap_or_else(|| format!("device {position}")),
			device,
		})
		.collect();
	if cfg!(windows) {
		// The exclusive driver opens the very endpoints the host driver lists.
		let exclusive: Vec<OutputDevice> = outputs
			.iter()
			.map(|output| OutputDevice {
				driver: Driver::WasapiExclusive,
				name: output.name.clone(),
				device: output.device.clone(),
			})
			.collect();
		outputs.extend(exclusive);
	}
	Ok(outputs)
}

/// The default mark belongs to the default device of the host driver alone (D-02).
fn is_default_output(driver: Driver, name: &str, default_name: Option<&str>) -> bool {
	driver == Driver::Host && default_name == Some(name)
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
				tracing::warn!(device = %output.label(), %source, "skipping audio device, cannot read its configuration");
				continue;
			}
		};
		// What cpal reports is the range of its own stream; an exclusive one has other limits, which
		// only opening it tells.
		let buffer_range = match (output.driver, config.buffer_size()) {
			(Driver::Host | Driver::Asio, SupportedBufferSize::Range { min, max }) => Some((*min, *max)),
			_ => None,
		};
		infos.push(AudioDeviceInfo {
			index,
			driver: output.driver,
			is_default: is_default_output(output.driver, &output.name, default_name.as_deref()),
			name: output.name,
			default_sample_rate: config.sample_rate(),
			buffer_range,
		});
	}
	Ok(infos)
}

pub fn default_output_device() -> Result<OutputDevice, AudioError> {
	let device = cpal::default_host().default_output_device().ok_or(AudioError::NoOutputDevice)?;
	Ok(OutputDevice {
		driver: Driver::Host,
		name: device_name(&device).unwrap_or_else(|| "unknown".to_owned()),
		device,
	})
}

/// Result of resolving the user's device choice.
pub struct SelectedDevice {
	pub device: OutputDevice,
	/// The selector was given but matched nothing, so the default device is used instead (logged as a warning).
	pub requested_missing: bool,
}

/// Picks the output for `selector`, which is matched against the whole `[Driver]: Device` (D-03);
/// with no selector or no match, the system default is used (F-04 fallback).
pub fn select_output_device(selector: Option<&DeviceSelector>) -> Result<SelectedDevice, AudioError> {
	let Some(selector) = selector else {
		return Ok(SelectedDevice {
			device: default_output_device()?,
			requested_missing: false,
		});
	};
	let mut outputs = enumerate_outputs()?;
	let labels: Vec<String> = outputs.iter().map(OutputDevice::label).collect();
	match selector.find(labels.iter().map(String::as_str)) {
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

/// The output at `index` in the enumeration order shown by `--list-devices` and the output list.
pub fn output_device_by_index(index: usize) -> Result<OutputDevice, AudioError> {
	enumerate_outputs()?.into_iter().nth(index).ok_or(AudioError::NoOutputDevice)
}

fn device_name(device: &Device) -> Option<String> {
	device.description().ok().map(|description| description.name().to_owned())
}

#[cfg(test)]
mod tests {
	use super::*;

	const SPEAKERS: &str = "Speakers (Realtek(R) Audio)";
	const HEADPHONES: &str = "Headphones (QCY H3 Stereo)";

	/// The list of a Windows machine with two devices: both through the host driver, then both taken
	/// exclusively, then an ASIO driver.
	fn labels() -> Vec<String> {
		vec![
			output_label(Driver::Host, HEADPHONES),
			output_label(Driver::Host, SPEAKERS),
			output_label(Driver::WasapiExclusive, HEADPHONES),
			output_label(Driver::WasapiExclusive, SPEAKERS),
			output_label(Driver::Asio, "Realtek ASIO"),
		]
	}

	fn found(selector: &DeviceSelector) -> Option<usize> {
		let labels = labels();
		selector.find(labels.iter().map(String::as_str))
	}

	#[test]
	fn an_output_is_named_by_its_driver_and_its_device() {
		assert_eq!(
			output_label(Driver::WasapiExclusive, SPEAKERS),
			"[WASAPI Exclusive]: Speakers (Realtek(R) Audio)"
		);
		assert_eq!(output_label(Driver::Asio, "Realtek ASIO"), "[ASIO]: Realtek ASIO");
		assert_eq!(output_label(Driver::Host, SPEAKERS), format!("[{HOST_DRIVER_NAME}]: {SPEAKERS}"));
	}

	#[test]
	fn a_device_name_from_an_old_config_finds_the_output_of_the_host_driver() {
		assert_eq!(found(&DeviceSelector::Name(SPEAKERS.to_owned())), Some(1));
		assert_eq!(found(&DeviceSelector::Name("qcy h3".to_owned())), Some(0));
	}

	#[test]
	fn a_whole_label_finds_the_output_of_its_own_driver() {
		let exclusive = output_label(Driver::WasapiExclusive, SPEAKERS);
		assert_eq!(found(&DeviceSelector::Name(exclusive)), Some(3));
		assert_eq!(found(&DeviceSelector::Name("[asio]".to_owned())), Some(4));
	}

	#[test]
	fn an_index_counts_the_outputs_of_every_driver() {
		assert_eq!(found(&DeviceSelector::Index(2)), Some(2));
		assert_eq!(found(&DeviceSelector::Index(5)), None);
	}

	#[test]
	fn only_the_default_device_of_the_host_driver_is_marked_as_the_default() {
		assert!(is_default_output(Driver::Host, SPEAKERS, Some(SPEAKERS)));
		assert!(!is_default_output(Driver::Host, HEADPHONES, Some(SPEAKERS)));
		assert!(!is_default_output(Driver::WasapiExclusive, SPEAKERS, Some(SPEAKERS)));
		assert!(!is_default_output(Driver::Host, SPEAKERS, None));
	}
}
