//! What the windows and the dropdowns exchange with the rest of the UI: what the preset window is asked
//! to show, and what a choice made in a list means for the session (spec F-01, F-03, F-06).

use crate::synth::PresetInfo;

/// What the preset window is asked to show: the presets of the loaded bank and the one in use
/// (`None` — the bank plays its first preset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetRequest {
	pub presets: Vec<PresetInfo>,
	pub current: Option<(u16, u8)>,
}

/// What the UI asks the session to do with a row chosen in a dropdown or in the preset window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
	ConnectMidi(usize),
	OpenAudio(usize),
	SelectPreset { bank: u16, program: u8 },
}

/// "bank:program name" as shown in the header and the preset list (spec §6).
pub fn preset_label(preset: &PresetInfo) -> String {
	format!("{:03}:{:03} {}", preset.bank, preset.program, preset.name)
}
