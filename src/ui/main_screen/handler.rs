//! The "Engine" section of the main screen: the choice between the built-in synthesizer and a
//! SoundFont, and the rows of the chosen one.

use gterm::buffer::ScreenBuffer;
use rust_i18n::t;

use super::{
	DISABLED, ERROR, GOOD, LABEL, MINUS, NOTHING, PLUS, SECOND_VALUE_X, SECTION_HEAD_ROWS, SYNTH_EXTRA_ROWS, TEXT, VALUE, VALUE_X, label,
	put, rule, section, sub_label,
};
use crate::config::EngineKind;
use crate::synth::{AdsrPreset, Wave};
use crate::ui::app::ButtonId;
use crate::ui::controls::{Controls, Look};
use crate::ui::paint::Pen;
use crate::ui::view::{NoticeTarget, View};

/// "─ ENGINE  [Synth] [Sound Font] ──" and, under it, the rows of the chosen engine; both end
/// with the effects row, which the two share.
pub(super) fn handler(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View, controls: &mut Controls) {
	let after_title = section(buf, y, &t!("main.section_handler", locale = locale));
	let chosen = |kind: EngineKind| if view.engine == kind { Look::Active } else { Look::Normal };
	let mut pen = Pen::at(buf, after_title + 1, y);
	put(
		&mut pen,
		controls,
		ButtonId::EngineSynth,
		&t!("main.engine_synth", locale = locale),
		chosen(EngineKind::Builtin),
	);
	pen.gap(1);
	put(
		&mut pen,
		controls,
		ButtonId::EngineSf2,
		&t!("main.engine_sf2", locale = locale),
		chosen(EngineKind::Sf2),
	);
	pen.gap(1);
	let after_buttons = pen.x;
	rule(buf, after_buttons, y);

	let rows_y = y + SECTION_HEAD_ROWS;
	let effects_y = match view.engine {
		EngineKind::Sf2 => {
			sound_font(buf, rows_y, locale, view, controls);
			rows_y + 2
		}
		EngineKind::Builtin => {
			synth(buf, rows_y, locale, view, controls);
			rows_y + 2 + SYNTH_EXTRA_ROWS
		}
	};
	let effects = t!("main.effects", locale = locale);
	label(buf, effects_y, &effects);
	let off = t!("mixer.off", locale = locale);
	let level = |value: Option<u8>| value.map_or_else(|| off.to_string(), |level| level.to_string());
	let sends = format!(
		"{} {}  {} {}",
		t!("mixer.reverb", locale = locale),
		level(view.reverb),
		t!("mixer.chorus", locale = locale),
		level(view.chorus)
	);
	let mut pen = Pen::at(buf, VALUE_X, effects_y);
	put(&mut pen, controls, ButtonId::Effects, &effects, Look::Normal);
	pen.gap(2).text(&sends, TEXT);
}

/// The four rows of the built-in synthesizer: the wave, the ready-made envelopes, the envelope's values.
fn synth(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View, controls: &mut Controls) {
	let timbre = &view.builtin;
	label(buf, y, &t!("main.wave", locale = locale));
	let mut pen = Pen::at(buf, VALUE_X, y);
	for (id, wave, key) in [
		(ButtonId::WaveSine, Wave::Sine, "main.wave_sine"),
		(ButtonId::WaveSaw, Wave::Saw, "main.wave_saw"),
		(ButtonId::WaveTriangle, Wave::Triangle, "main.wave_triangle"),
	] {
		let look = if timbre.wave == wave { Look::Active } else { Look::Normal };
		put(&mut pen, controls, id, &t!(key, locale = locale), look);
		pen.gap(1);
	}

	label(buf, y + 1, "ADSR");
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	pen.text(&t!("main.adsr_preset", locale = locale), LABEL).gap(1);
	for (id, preset, key) in [
		(ButtonId::AdsrPiano, AdsrPreset::Piano, "main.adsr_piano"),
		(ButtonId::AdsrHarpsichord, AdsrPreset::Harpsichord, "main.adsr_harpsichord"),
		(ButtonId::AdsrOrgan, AdsrPreset::Organ, "main.adsr_organ"),
		(ButtonId::AdsrFlute, AdsrPreset::Flute, "main.adsr_flute"),
	] {
		let look = if timbre.adsr_preset == preset { Look::Active } else { Look::Normal };
		put(&mut pen, controls, id, &t!(key, locale = locale), look);
		pen.gap(1);
	}
	// Values set by hand belong to no preset (spec v1.2, F-22).
	if timbre.adsr_preset == AdsrPreset::Custom {
		pen.gap(1).text(&t!("main.adsr_custom", locale = locale), VALUE);
	}

	sub_label(buf, y + 2, &t!("main.adsr_values", locale = locale));
	let seconds = |value: f32| t!("main.seconds", locale = locale, value = format!("{value:.3}")).into_owned();
	let per_cent = |share: f32| format!("{:>3}%", (share * 100.0).round() as u32);
	for (row, x, name, value, down, up) in [
		(
			y + 2,
			VALUE_X,
			"Attack:",
			seconds(timbre.attack),
			ButtonId::AttackDown,
			ButtonId::AttackUp,
		),
		(
			y + 2,
			SECOND_VALUE_X,
			"Decay:",
			seconds(timbre.decay),
			ButtonId::DecayDown,
			ButtonId::DecayUp,
		),
		(
			y + 3,
			VALUE_X,
			"Sustain:",
			per_cent(timbre.sustain),
			ButtonId::SustainDown,
			ButtonId::SustainUp,
		),
		(
			y + 3,
			SECOND_VALUE_X,
			"Release:",
			seconds(timbre.release),
			ButtonId::ReleaseDown,
			ButtonId::ReleaseUp,
		),
	] {
		let mut pen = Pen::at(buf, x, row);
		pen.text(name, LABEL).gap(1);
		put(&mut pen, controls, down, MINUS, Look::Normal);
		pen.gap(1).text(&value, VALUE).gap(1);
		put(&mut pen, controls, up, PLUS, Look::Normal);
	}
}

/// The two rows of a SoundFont: the bank file and the preset.
fn sound_font(buf: &mut ScreenBuffer, y: u16, locale: &str, view: &View, controls: &mut Controls) {
	let sf2_file = t!("main.sf2_file", locale = locale);
	label(buf, y, &sf2_file);
	let mut pen = Pen::at(buf, VALUE_X, y);
	put(&mut pen, controls, ButtonId::Sf2File, &sf2_file, Look::Normal);
	pen.gap(2);
	let sf2_error = view.line_error(NoticeTarget::Sf2);
	match (&view.bank, sf2_error) {
		(Some(bank), None) => {
			pen.text(&bank.file_name, TEXT)
				.gap(2)
				.text(&t!("header.audio_ok", locale = locale), GOOD);
		}
		(Some(bank), Some(error)) => {
			pen.text(&bank.file_name, TEXT).gap(2).text(error, ERROR);
		}
		(None, Some(error)) => {
			pen.text(error, ERROR);
		}
		(None, None) => {
			pen.text(&t!("header.sf2_missing", locale = locale), ERROR);
		}
	}

	let preset = t!("main.preset", locale = locale);
	label(buf, y + 1, &preset);
	let mut pen = Pen::at(buf, VALUE_X, y + 1);
	match &view.bank {
		Some(bank) => {
			put(&mut pen, controls, ButtonId::Preset, &preset, Look::Normal);
			pen.gap(2).text(&bank.preset_label, TEXT);
		}
		None => {
			put(&mut pen, controls, ButtonId::Preset, &preset, Look::Disabled);
			pen.gap(2).text(NOTHING, DISABLED);
		}
	}
}
