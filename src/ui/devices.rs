//! The rows of the device dropdowns of the main screen (spec v1.3, F-26.1) and what choosing one asks
//! for. The list is taken from the system when the dropdown is opened; a row is told to the session
//! by the index the system gave the device, not by its place in the list.

use super::dialog::Choice;
use crate::audio::AudioDeviceInfo;
use crate::midi::MidiPortInfo;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceRows {
	names: Vec<String>,
	choices: Vec<Choice>,
}

impl DeviceRows {
	pub fn midi(ports: &[MidiPortInfo]) -> Self {
		Self {
			names: ports.iter().map(|port| port.name.clone()).collect(),
			choices: ports.iter().map(|port| Choice::ConnectMidi(port.index)).collect(),
		}
	}

	pub fn audio(devices: &[AudioDeviceInfo]) -> Self {
		Self {
			names: devices.iter().map(|device| device.name.clone()).collect(),
			choices: devices.iter().map(|device| Choice::OpenAudio(device.index)).collect(),
		}
	}

	/// What the rows of the list say, top to bottom.
	pub fn names(&self) -> &[String] {
		&self.names
	}

	/// What choosing a row asks the session to do; `None` for a row the list does not have.
	pub fn choice(&self, row: usize) -> Option<Choice> {
		self.choices.get(row).cloned()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn midi_rows_are_the_port_names_and_choosing_one_connects_the_port_by_its_index() {
		let ports = [
			MidiPortInfo {
				index: 3,
				name: "KeyStep".into(),
			},
			MidiPortInfo {
				index: 7,
				name: "loopMIDI".into(),
			},
		];
		let rows = DeviceRows::midi(&ports);
		assert_eq!(rows.names(), ["KeyStep", "loopMIDI"]);
		assert_eq!(rows.choice(1), Some(Choice::ConnectMidi(7)));
		assert_eq!(rows.choice(2), None);
	}

	#[test]
	fn audio_rows_are_the_device_names_and_choosing_one_opens_the_device_by_its_index() {
		let devices = [
			AudioDeviceInfo {
				index: 0,
				name: "Speakers".into(),
				is_default: true,
				default_sample_rate: 48_000,
				buffer_range: Some((128, 2048)),
			},
			AudioDeviceInfo {
				index: 4,
				name: "HDMI".into(),
				is_default: false,
				default_sample_rate: 44_100,
				buffer_range: None,
			},
		];
		let rows = DeviceRows::audio(&devices);
		assert_eq!(rows.names(), ["Speakers", "HDMI"]);
		assert_eq!(rows.choice(0), Some(Choice::OpenAudio(0)));
		assert_eq!(rows.choice(1), Some(Choice::OpenAudio(4)));
	}

	#[test]
	fn an_empty_list_has_no_rows_and_no_choices() {
		let rows = DeviceRows::default();
		assert!(rows.names().is_empty());
		assert_eq!(rows.choice(0), None);
	}
}
