//! Applying UI requests to the running session: mixer, effects, presets, panic, text-keyboard notes.
//! Mechanically split from `session.rs`; everything here talks to the audio thread through the command ring.

use std::path::PathBuf;
use std::sync::Arc;

use ringbuf::HeapProd;
use ringbuf::traits::Producer;

use super::{MIDI_CHANNELS, Notice, Session};
use crate::audio::Command;
use crate::audio::mixer::{MASTER_STEP, MASTER_STEP_FAST};
use crate::config::{AdsrField, EngineKind, RecordFormat, SynthConfig};
use crate::midi::{MidiEvent, controller};
use crate::synth::{AdsrPreset, BuiltinSynth, PresetInfo, RustySynthEngine, SoundFontBank, SynthEngine, SynthError, Timbre, Wave};

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

/// What the master keys do (spec F-10, master part; channel strips left in v1.1 — ADR 0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerAction {
	MasterUp,
	MasterDown,
	MasterUpFast,
	MasterDownFast,
	MasterMute,
}

impl Session {
	/// Applies a master key (F-10): the gain atomic the audio thread reads, remembered in the config.
	pub fn apply_mixer(&mut self, action: MixerAction) {
		match action {
			MixerAction::MasterUp => self.mixer.adjust_master(MASTER_STEP),
			MixerAction::MasterDown => self.mixer.adjust_master(-MASTER_STEP),
			MixerAction::MasterUpFast => self.mixer.adjust_master(MASTER_STEP_FAST),
			MixerAction::MasterDownFast => self.mixer.adjust_master(-MASTER_STEP_FAST),
			MixerAction::MasterMute => self.mixer.toggle_master_mute(),
		}
		self.shared.set_master_gain(self.mixer.master_gain());
		self.config.mixer.master = self.mixer.master;
	}

	/// Changes the effects (F-13). With a SoundFont, turning the last effect off or the first one on
	/// rebuilds the engine, because rustysynth decides at construction whether the effect units exist;
	/// otherwise, and always with the built-in synthesizer, only the sends change.
	pub fn apply_effects(&mut self, action: EffectsAction) {
		let had_effects = self.config.synth.reverb || self.config.synth.chorus;
		match action {
			EffectsAction::ToggleReverb => self.config.synth.reverb = !self.config.synth.reverb,
			EffectsAction::ToggleChorus => self.config.synth.chorus = !self.config.synth.chorus,
			EffectsAction::Set(settings) => settings.apply_to(&mut self.config.synth),
		}
		let has_effects = self.config.synth.reverb || self.config.synth.chorus;
		if needs_new_engine(self.config.synth.engine, had_effects, has_effects) {
			if let Err(error) = self.install_engine() {
				self.notice = Some(Notice::Sf2Error(error.to_string()));
			}
		} else if let Some(commands) = self.commands.as_mut() {
			push_effect_sends(commands, &self.config.synth);
		}
	}

	/// Switches the processor (spec v1.2, F-22): the new engine replaces the old one on the audio
	/// thread, and the notes that were sounding end with it. Without a bank the SoundFont processor is silent.
	pub fn set_engine(&mut self, kind: EngineKind) {
		if self.config.synth.engine == kind {
			return;
		}
		self.config.synth.engine = kind;
		if let Err(error) = self.install_engine() {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
		}
	}

	/// The wave of the built-in synthesizer, for the notes that start from now on.
	pub fn set_wave(&mut self, wave: Wave) {
		self.config.synth.builtin.wave = wave;
		self.timbre.set_wave(wave);
	}

	/// A ready-made envelope for the built-in synthesizer.
	pub fn set_adsr_preset(&mut self, preset: AdsrPreset) {
		self.config.synth.builtin.apply_preset(preset);
		self.timbre.set_adsr(self.config.synth.builtin.adsr());
	}

	/// One value of the envelope moved by a step; the envelope is the user's own from then on.
	pub fn adjust_adsr(&mut self, field: AdsrField, steps: i8) {
		self.config.synth.builtin.adjust(field, steps);
		self.timbre.set_adsr(self.config.synth.builtin.adsr());
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

	/// Notes played in the window (F-16, F-24): same path as hardware MIDI, via the command ring
	/// (ADR 0009), as the notes of the keyboard source.
	pub fn send_notes(&mut self, events: &[MidiEvent]) {
		if let Some(commands) = self.commands.as_mut() {
			push_played(commands, events);
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

	/// Where takes go: `None` means the OS music directory (spec v1.1 §4.3).
	pub fn set_record_dir(&mut self, dir: Option<PathBuf>) {
		self.config.set_record_dir(dir);
	}

	/// The format of the next take; a running take keeps its own.
	pub fn set_record_format(&mut self, format: RecordFormat) {
		self.config.record.format = format;
	}

	/// F5: the preset for every channel except percussion (spec v1.1 §4.1, ADR 0011), remembered in the config.
	pub fn select_preset(&mut self, bank: u16, program: u8) {
		self.preset = Some((bank, program));
		self.config.synth.preset = self.preset;
		if let Some(commands) = self.commands.as_mut() {
			push_global_preset(commands, bank, program);
		}
	}

	/// The user's F5 choice; `None` = the first preset of the bank.
	pub fn preset_for(&self) -> Option<(u16, u8)> {
		self.preset
	}

	/// The preset shown in the header: the chosen one, or the engine default (first preset of the bank).
	pub fn preset_info(&self) -> Option<&PresetInfo> {
		let presets = self.bank.as_ref()?.bank.presets();
		match self.preset {
			Some((bank, program)) => presets.iter().find(|preset| (preset.bank, preset.program) == (bank, program)),
			None => presets.first(),
		}
	}
}

/// MIDI channel 10 (0-based 9): percussion in General MIDI and in SF2 (bank 128), left out of the global preset.
pub const PERCUSSION_CHANNEL: u8 = 9;

/// Whether a change of the effect switches calls for a new engine. It does for a SoundFont when the
/// first effect goes on or the last one goes off: rustysynth decides at construction whether its effect
/// units exist. The built-in synthesizer always has them and only follows the send levels.
pub(super) fn needs_new_engine(kind: EngineKind, had_effects: bool, has_effects: bool) -> bool {
	kind == EngineKind::Sf2 && had_effects != has_effects
}

/// The engine for the chosen processor. A SoundFont needs its bank: without one there is no engine,
/// which is silence. The built-in synthesizer needs nothing but the timbre it shares with the UI.
pub(super) fn build_engine(
	kind: EngineKind,
	bank: Option<&SoundFontBank>,
	timbre: &Arc<Timbre>,
	sample_rate: u32,
	effects: bool,
) -> Result<Option<Box<dyn SynthEngine>>, SynthError> {
	Ok(match (kind, bank) {
		(EngineKind::Builtin, _) => Some(Box::new(BuiltinSynth::new(sample_rate, Arc::clone(timbre)))),
		(EngineKind::Sf2, Some(bank)) => Some(Box::new(RustySynthEngine::new(bank, sample_rate, effects)?)),
		(EngineKind::Sf2, None) => None,
	})
}

/// What was played in the window goes as `Command::Play`, so the audio thread counts it among the notes
/// of the keyboard source; an event that does not fit the ring is dropped and logged.
pub(super) fn push_played(commands: &mut HeapProd<Command>, events: &[MidiEvent]) {
	for event in events {
		if commands.try_push(Command::Play(*event)).is_err() {
			tracing::warn!("command ring full, text keyboard event dropped");
		}
	}
}

/// Bank Select + Program Change on every melodic channel (spec v1.1 §4.1): 30 ring pushes.
pub(super) fn push_global_preset(commands: &mut HeapProd<Command>, bank: u16, program: u8) {
	for channel in (0..MIDI_CHANNELS as u8).filter(|channel| *channel != PERCUSSION_CHANNEL) {
		push_preset(commands, channel, bank, program);
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
	use ringbuf::HeapRb;
	use ringbuf::traits::{Consumer, Split};

	use super::*;

	/// The global preset reaches channels 1 and 16 and skips channel 10 (spec v1.1 §7, criterion 3).
	#[test]
	fn global_preset_selects_bank_and_program_on_every_channel_but_percussion() {
		let (mut commands, mut received) = HeapRb::<Command>::new(64).split();
		push_global_preset(&mut commands, 8, 5);
		let mut programs = Vec::new();
		let mut bank_selects = Vec::new();
		while let Some(command) = received.try_pop() {
			match command {
				Command::Midi(MidiEvent::ProgramChange { channel, program }) => programs.push((channel, program)),
				Command::Midi(MidiEvent::ControlChange {
					channel,
					controller: controller::BANK_SELECT,
					value,
				}) => bank_selects.push((channel, value)),
				_ => panic!("unexpected command: only Bank Select and Program Change belong here"),
			}
		}
		let expected_channels: Vec<u8> = (0..16).filter(|channel| *channel != PERCUSSION_CHANNEL).collect();
		assert_eq!(programs.iter().map(|(channel, _)| *channel).collect::<Vec<_>>(), expected_channels);
		assert_eq!(
			bank_selects.iter().map(|(channel, _)| *channel).collect::<Vec<_>>(),
			expected_channels
		);
		assert!(programs.iter().all(|(_, program)| *program == 5));
		assert!(bank_selects.iter().all(|(_, bank)| *bank == 8));
		assert!(!programs.iter().any(|(channel, _)| *channel == 9), "channel 10 keeps its drum kit");
	}

	/// RMS of what an engine renders for one note held a tenth of a second.
	fn loudness_of_a_note(engine: &mut dyn SynthEngine) -> f32 {
		engine.note_on(0, 60, 100);
		let mut left = vec![0.0f32; 4_800];
		let mut right = vec![0.0f32; 4_800];
		engine.render(&mut left, &mut right);
		(left.iter().map(|sample| sample * sample).sum::<f32>() / left.len() as f32).sqrt()
	}

	#[test]
	fn the_builtin_synth_needs_no_bank_and_a_sound_font_is_silent_without_one() {
		let timbre = Arc::new(Timbre::new(Wave::Saw, crate::config::BuiltinConfig::default().adsr()));
		let mut engine = build_engine(EngineKind::Builtin, None, &timbre, 48_000, true)
			.expect("nothing to fail")
			.expect("an engine without a bank");
		assert_eq!(engine.max_voices(), 64);
		assert!(loudness_of_a_note(engine.as_mut()) > 0.01);
		// The engine shares the timbre it was given: a change reaches its next note.
		assert_eq!(Arc::strong_count(&timbre), 2);

		assert!(
			build_engine(EngineKind::Sf2, None, &timbre, 48_000, true)
				.expect("no error")
				.is_none()
		);

		let bank = SoundFontBank::load(std::path::Path::new(crate::synth::test_support::TEST_BANK_PATH)).expect("test bank");
		let mut engine = build_engine(EngineKind::Sf2, Some(&bank), &timbre, 48_000, true)
			.expect("the bank fits the rate")
			.expect("an engine for the bank");
		assert!(loudness_of_a_note(engine.as_mut()) > 0.01);
		// With a bank at hand the built-in synthesizer is still the built-in synthesizer.
		let engine = build_engine(EngineKind::Builtin, Some(&bank), &timbre, 48_000, false).expect("no error");
		assert_eq!(engine.map(|engine| engine.max_voices()), Some(64));
		assert_eq!(Arc::strong_count(&timbre), 2, "the dropped engines let go of the timbre");
		// A rate the SoundFont engine cannot work at is an error, not a silent processor.
		assert!(build_engine(EngineKind::Sf2, Some(&bank), &timbre, 1_000, true).is_err());
	}

	#[test]
	fn only_a_sound_font_needs_a_new_engine_when_its_effects_appear_or_go() {
		assert!(needs_new_engine(EngineKind::Sf2, false, true), "the first effect goes on");
		assert!(needs_new_engine(EngineKind::Sf2, true, false), "the last effect goes off");
		assert!(
			!needs_new_engine(EngineKind::Sf2, true, true),
			"a level changed, or one of two effects"
		);
		assert!(!needs_new_engine(EngineKind::Sf2, false, false));
		for (had, has) in [(false, true), (true, false), (true, true), (false, false)] {
			assert!(
				!needs_new_engine(EngineKind::Builtin, had, has),
				"the built-in synthesizer keeps its engine"
			);
		}
	}

	#[test]
	fn notes_played_in_the_window_travel_as_play_commands_in_order() {
		let (mut commands, mut received) = HeapRb::<Command>::new(2).split();
		let played = [
			MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100,
			},
			MidiEvent::NoteOff { channel: 0, key: 60 },
			MidiEvent::NoteOff { channel: 0, key: 62 },
		];
		push_played(&mut commands, &played);
		let mut arrived = Vec::new();
		while let Some(command) = received.try_pop() {
			match command {
				Command::Play(event) => arrived.push(event),
				_ => panic!("a played note is not a service message"),
			}
		}
		assert_eq!(
			arrived,
			played[..2],
			"what does not fit the ring is dropped, the rest keeps its order"
		);
	}

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
