//! The modal dialogs built on `Selector` (spec F-01, F-03, F-06) and what a choice means.

use std::path::PathBuf;

use rust_i18n::t;

use super::selector::{Selector, SelectorItem};
use crate::audio::AudioDeviceInfo;
use crate::config::RecordFormat;
use crate::midi::MidiPortInfo;
use crate::synth::PresetInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
	MidiInput,
	AudioOutput,
	Preset,
}

impl DialogKind {
	pub fn title_key(self) -> &'static str {
		match self {
			Self::MidiInput => "dialog.midi_title",
			Self::AudioOutput => "dialog.audio_title",
			Self::Preset => "dialog.preset_title",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
	pub kind: DialogKind,
	pub selector: Selector,
	/// Payload needed to turn a chosen row back into a request.
	choices: Choices,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Choices {
	MidiPorts(Vec<usize>),
	AudioDevices(Vec<usize>),
	Presets(Vec<(u16, u8)>),
}

/// What the UI asks the session to do after a dialog closes with Enter (the F7 dialog reports here too).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
	ConnectMidi(usize),
	OpenAudio(usize),
	SelectPreset {
		bank: u16,
		program: u8,
	},
	/// `dir` `None` = the OS music folder (spec v1.1 §4.3).
	SetRecording {
		dir: Option<PathBuf>,
		format: RecordFormat,
	},
}

impl Dialog {
	pub fn midi(ports: &[MidiPortInfo], connected: Option<&str>) -> Self {
		let items = ports
			.iter()
			.map(|port| SelectorItem {
				label: port.name.clone(),
				detail: None,
			})
			.collect();
		let current = ports.iter().position(|port| Some(port.name.as_str()) == connected);
		Self {
			kind: DialogKind::MidiInput,
			selector: Selector::new(items, current),
			choices: Choices::MidiPorts(ports.iter().map(|port| port.index).collect()),
		}
	}

	pub fn audio(devices: &[AudioDeviceInfo], current_name: Option<&str>, locale: &str) -> Self {
		let items = devices
			.iter()
			.map(|device| {
				let buffer = match device.buffer_range {
					Some((min, max)) => t!("dialog.audio_buffer_range", locale = locale, min = min, max = max).into_owned(),
					None => t!("dialog.audio_buffer_unknown", locale = locale).into_owned(),
				};
				SelectorItem {
					label: device.name.clone(),
					detail: Some(format!("{} Hz, {buffer}", device.default_sample_rate)),
				}
			})
			.collect();
		let current = devices.iter().position(|device| Some(device.name.as_str()) == current_name);
		Self {
			kind: DialogKind::AudioOutput,
			selector: Selector::new(items, current),
			choices: Choices::AudioDevices(devices.iter().map(|device| device.index).collect()),
		}
	}

	pub fn preset(presets: &[PresetInfo], current: Option<(u16, u8)>) -> Self {
		let items = presets
			.iter()
			.map(|preset| SelectorItem {
				label: preset_label(preset),
				detail: None,
			})
			.collect();
		let current =
			current.and_then(|(bank, program)| presets.iter().position(|preset| (preset.bank, preset.program) == (bank, program)));
		Self {
			kind: DialogKind::Preset,
			selector: Selector::new(items, current),
			choices: Choices::Presets(presets.iter().map(|preset| (preset.bank, preset.program)).collect()),
		}
	}

	/// Translates the chosen row (original index) into a request.
	pub fn choice(&self, row: usize) -> Option<Choice> {
		match &self.choices {
			Choices::MidiPorts(indices) => indices.get(row).map(|index| Choice::ConnectMidi(*index)),
			Choices::AudioDevices(indices) => indices.get(row).map(|index| Choice::OpenAudio(*index)),
			Choices::Presets(presets) => presets.get(row).map(|(bank, program)| Choice::SelectPreset {
				bank: *bank,
				program: *program,
			}),
		}
	}
}

/// "bank:program name" as shown in the header and the preset list (spec §6).
pub fn preset_label(preset: &PresetInfo) -> String {
	format!("{:03}:{:03} {}", preset.bank, preset.program, preset.name)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::audio::Backend;

	#[test]
	fn midi_dialog_marks_the_connected_port_and_maps_rows_to_port_indices() {
		let ports = vec![
			MidiPortInfo {
				index: 0,
				name: "A".into(),
			},
			MidiPortInfo {
				index: 1,
				name: "B".into(),
			},
		];
		let dialog = Dialog::midi(&ports, Some("B"));
		assert_eq!(dialog.selector.current(), Some(1));
		assert_eq!(dialog.choice(0), Some(Choice::ConnectMidi(0)));
		assert_eq!(dialog.choice(5), None);
	}

	#[test]
	fn audio_dialog_describes_rate_and_buffer_range() {
		let devices = vec![
			AudioDeviceInfo {
				index: 0,
				name: "Speakers".into(),
				backend: Backend::Default,
				is_default: true,
				default_sample_rate: 48_000,
				buffer_range: Some((128, 2048)),
			},
			AudioDeviceInfo {
				index: 1,
				name: "HDMI".into(),
				backend: Backend::Default,
				is_default: false,
				default_sample_rate: 44_100,
				buffer_range: None,
			},
		];
		let dialog = Dialog::audio(&devices, Some("HDMI"), "en");
		assert_eq!(dialog.selector.current(), Some(1));
		assert_eq!(dialog.selector.item(0).detail.as_deref(), Some("48000 Hz, buffer 128..2048 frames"));
		assert_eq!(dialog.selector.item(1).detail.as_deref(), Some("44100 Hz, buffer range unknown"));
		assert_eq!(dialog.choice(1), Some(Choice::OpenAudio(1)));
	}

	#[test]
	fn preset_dialog_uses_bank_program_labels() {
		let presets = vec![
			PresetInfo {
				bank: 0,
				program: 0,
				name: "Grand Piano".into(),
			},
			PresetInfo {
				bank: 128,
				program: 0,
				name: "Standard Kit".into(),
			},
		];
		let dialog = Dialog::preset(&presets, Some((128, 0)));
		assert_eq!(dialog.selector.item(0).label, "000:000 Grand Piano");
		assert_eq!(dialog.selector.current(), Some(1));
		assert_eq!(dialog.choice(1), Some(Choice::SelectPreset { bank: 128, program: 0 }));
	}
}
