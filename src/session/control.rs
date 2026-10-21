//! Applying UI requests to the running session: mixer, effects, presets, panic, text-keyboard notes.
//! Mechanically split from `session.rs`; everything here talks to the audio thread through the command ring.

use ringbuf::HeapProd;
use ringbuf::traits::Producer;

use super::{MIDI_CHANNELS, Notice, Session};
use crate::audio::Command;
use crate::audio::mixer::{CHANNEL_STEP, MASTER_STEP, MASTER_STEP_FAST};
use crate::config::{RecordFormat, SynthConfig};
use crate::midi::{MidiEvent, controller};
use crate::synth::PresetInfo;

/// Reverb / chorus switches (F-13): Alt+R / Alt+C toggle one effect, the F6 dialog sets everything at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectsAction {
	ToggleReverb,
	ToggleChorus,
	Set(EffectsSettings),
}

/// The effect state the F6 dialog edits: switches plus send levels (CC 91 / CC 93, 0..=127).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectsSettings {
	pub reverb: bool,
	pub chorus: bool,
	pub reverb_level: u8,
	pub chorus_level: u8,
}

impl EffectsSettings {
	pub fn from_config(synth: &SynthConfig) -> Self {
		Self {
			reverb: synth.reverb,
			chorus: synth.chorus,
			reverb_level: synth.reverb_level.min(127),
			chorus_level: synth.chorus_level.min(127),
		}
	}

	fn apply_to(self, synth: &mut SynthConfig) {
		synth.reverb = self.reverb;
		synth.chorus = self.chorus;
		synth.reverb_level = self.reverb_level;
		synth.chorus_level = self.chorus_level;
	}
}

/// What the mixer keys do (spec F-10, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerAction {
	MasterUp,
	MasterDown,
	MasterUpFast,
	MasterDownFast,
	MasterMute,
	/// 0-based channel.
	SelectChannel(u8),
	ChannelUp,
	ChannelDown,
	ChannelMute,
	ChannelSolo,
}

impl Session {
	/// Applies a mixer key (F-10): master through the gain atomic, channels through CC 7 on the command ring.
	pub fn apply_mixer(&mut self, action: MixerAction) {
		let channel = usize::from(self.current_channel);
		match action {
			MixerAction::MasterUp => self.mixer.adjust_master(MASTER_STEP),
			MixerAction::MasterDown => self.mixer.adjust_master(-MASTER_STEP),
			MixerAction::MasterUpFast => self.mixer.adjust_master(MASTER_STEP_FAST),
			MixerAction::MasterDownFast => self.mixer.adjust_master(-MASTER_STEP_FAST),
			MixerAction::MasterMute => self.mixer.toggle_master_mute(),
			MixerAction::SelectChannel(index) => {
				if usize::from(index) < MIDI_CHANNELS {
					self.current_channel = index;
				}
				return;
			}
			MixerAction::ChannelUp => self.mixer.adjust_channel(channel, CHANNEL_STEP),
			MixerAction::ChannelDown => self.mixer.adjust_channel(channel, -CHANNEL_STEP),
			MixerAction::ChannelMute => self.mixer.toggle_mute(channel),
			MixerAction::ChannelSolo => self.mixer.toggle_solo(channel),
		}
		self.shared.set_master_gain(self.mixer.master_gain());
		self.config.mixer.master = self.mixer.master;
		if let Some(commands) = self.commands.as_mut() {
			push_channel_volumes(commands, &self.mixer.effective_volumes());
		}
	}

	/// Changes the effects (F-13). Turning the last effect off or the first one on rebuilds the engine,
	/// because rustysynth decides at construction whether the effect units exist; otherwise only the sends change.
	pub fn apply_effects(&mut self, action: EffectsAction) {
		let had_effects = self.config.synth.reverb || self.config.synth.chorus;
		match action {
			EffectsAction::ToggleReverb => self.config.synth.reverb = !self.config.synth.reverb,
			EffectsAction::ToggleChorus => self.config.synth.chorus = !self.config.synth.chorus,
			EffectsAction::Set(settings) => settings.apply_to(&mut self.config.synth),
		}
		let has_effects = self.config.synth.reverb || self.config.synth.chorus;
		if has_effects != had_effects {
			if let Err(error) = self.install_engine() {
				self.notice = Some(Notice::Sf2Error(error.to_string()));
			}
		} else if let Some(commands) = self.commands.as_mut() {
			push_effect_sends(commands, &self.config.synth);
		}
	}

	/// What the F6 dialog starts from.
	pub fn effects(&self) -> EffectsSettings {
		EffectsSettings::from_config(&self.config.synth)
	}

	/// Current send levels for the UI: `None` when the effect is off.
	pub fn reverb_level(&self) -> Option<u8> {
		self.config.synth.reverb.then_some(self.config.synth.reverb_level.min(127))
	}

	pub fn chorus_level(&self) -> Option<u8> {
		self.config.synth.chorus.then_some(self.config.synth.chorus_level.min(127))
	}

	/// F-11: All Notes Off + All Sound Off on every channel.
	pub fn panic(&mut self) {
		let Some(commands) = self.commands.as_mut() else {
			return;
		};
		for channel in 0..MIDI_CHANNELS as u8 {
			let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
				channel,
				controller: controller::ALL_NOTES_OFF,
				value: 0,
			}));
			let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
				channel,
				controller: controller::ALL_SOUND_OFF,
				value: 0,
			}));
		}
	}

	/// Notes from the text keyboard (F-16): same path as hardware MIDI, via the command ring (ADR 0009).
	pub fn send_notes(&mut self, events: &[MidiEvent]) {
		let Some(commands) = self.commands.as_mut() else {
			return;
		};
		for event in events {
			if commands.try_push(Command::Midi(*event)).is_err() {
				tracing::warn!("command ring full, text keyboard event dropped");
			}
		}
	}

	/// `[` / `]`: transposition in semitones, clamped to ±12 (F-15).
	pub fn adjust_transpose(&mut self, delta: i8) {
		self.transpose = (i16::from(self.transpose) + i16::from(delta)).clamp(-12, 12) as i8;
		self.shared.set_transpose(self.transpose);
	}

	pub fn omni_channel(&self) -> Option<u8> {
		self.shared.omni_channel()
	}

	pub fn set_record_format(&mut self, format: RecordFormat) {
		self.config.record.format = format;
	}

	/// Chooses the preset for `channel` (F-06) by sending Bank Select + Program Change down the command ring.
	pub fn select_preset(&mut self, channel: u8, bank: u16, program: u8) {
		let Some(slot) = self.channel_presets.get_mut(usize::from(channel)) else {
			return;
		};
		*slot = Some((bank, program));
		if let Some(commands) = self.commands.as_mut() {
			push_preset(commands, channel, bank, program);
		}
	}

	pub fn preset_for(&self, channel: u8) -> Option<(u16, u8)> {
		self.channel_presets.get(usize::from(channel)).copied().flatten()
	}

	/// The preset shown for `channel`: the chosen one, or the engine default (first preset of the bank).
	pub fn preset_info(&self, channel: u8) -> Option<&PresetInfo> {
		let presets = self.bank.as_ref()?.bank.presets();
		match self.preset_for(channel) {
			Some((bank, program)) => presets.iter().find(|preset| (preset.bank, preset.program) == (bank, program)),
			None => presets.first(),
		}
	}
}

/// CC 91 / CC 93 for all channels: the configured level while the effect is on, 0 when off (F-13).
pub(super) fn push_effect_sends(commands: &mut HeapProd<Command>, synth: &SynthConfig) {
	let reverb = if synth.reverb { synth.reverb_level.min(127) } else { 0 };
	let chorus = if synth.chorus { synth.chorus_level.min(127) } else { 0 };
	for channel in 0..MIDI_CHANNELS as u8 {
		let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
			channel,
			controller: controller::REVERB_SEND,
			value: reverb,
		}));
		let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
			channel,
			controller: controller::CHORUS_SEND,
			value: chorus,
		}));
	}
}

/// CC 7 for all 16 channels; cheap enough to resend on every mixer change (16 ring pushes).
pub(super) fn push_channel_volumes(commands: &mut HeapProd<Command>, volumes: &[u8; MIDI_CHANNELS]) {
	for (channel, volume) in volumes.iter().enumerate() {
		let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
			channel: channel as u8,
			controller: controller::VOLUME,
			value: *volume,
		}));
	}
}

/// Bank Select (CC 0, 7-bit; the percussion bank 128 is implied by channel 10 in SF2) followed by Program Change.
pub(super) fn push_preset(commands: &mut HeapProd<Command>, channel: u8, bank: u16, program: u8) {
	let bank_msb = u8::try_from(bank & 0x7F).unwrap_or(0);
	let _ = commands.try_push(Command::Midi(MidiEvent::ControlChange {
		channel,
		controller: controller::BANK_SELECT,
		value: bank_msb,
	}));
	let _ = commands.try_push(Command::Midi(MidiEvent::ProgramChange { channel, program }));
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn effects_settings_round_trip_through_the_config_and_clamp_levels() {
		let mut synth = SynthConfig {
			reverb: false,
			chorus: true,
			reverb_level: 200,
			chorus_level: 7,
			..SynthConfig::default()
		};
		let settings = EffectsSettings::from_config(&synth);
		assert_eq!(
			settings,
			EffectsSettings {
				reverb: false,
				chorus: true,
				reverb_level: 127,
				chorus_level: 7
			}
		);
		EffectsSettings {
			reverb: true,
			chorus: false,
			reverb_level: 40,
			chorus_level: 0,
		}
		.apply_to(&mut synth);
		assert!(synth.reverb && !synth.chorus);
		assert_eq!((synth.reverb_level, synth.chorus_level), (40, 0));
		assert_eq!(synth.sf2, None, "other synth fields untouched");
	}
}
