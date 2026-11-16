//! The effects window (spec v1.3, F-26.3; v1.1 §4.2): reverb and chorus — a switch and a send level
//! each — and the `[OK]` / `[Cancel]` buttons. A change is applied at once, so it is heard while the
//! window is open; `[OK]` keeps it, `[Cancel]`, Esc and a click outside go back to what the window
//! opened with.
//!
//! Built from gterm-widgets controls: the checkboxes, the sliders, the Tab ring and the mouse come
//! from the library.

use gterm_widgets::widgets::{Block, Button, ButtonState, Checkbox, CheckboxState, Paragraph, Slider, SliderState};
use gterm_widgets::{Constraint, Layout, Line, Modal, Routed, Style, Ui};
use rust_i18n::t;

use super::paint::{BACKGROUND, HINT, LABEL, SELECTED, TEXT, VALUE};
use crate::session::EffectsSettings;

/// Size of the window in cells; it shrinks to the grid when the window is smaller.
pub const WIDTH: u16 = 64;
pub const HEIGHT: u16 = 6;
/// The arrows and the wheel move a send level by this much.
pub const LEVEL_STEP: i64 = 5;
/// A send level is a MIDI controller value (CC 91 / CC 93).
pub const MAX_LEVEL: u8 = 127;
/// Cells of the switch with the name of its effect, and of the level read-out after the slider.
const NAME_CELLS: u16 = 14;
const VALUE_CELLS: u16 = 4;

/// What the controls of the window report. A switch and a slider hold their new state themselves, so
/// one message serves them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectsMsg {
	Changed,
	Accept,
	Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectsOutcome {
	Pending,
	/// Apply now, keep the window open.
	Changed(EffectsSettings),
	/// `[OK]`: apply and save.
	Accepted(EffectsSettings),
	/// The window is left without `[OK]`: the state to go back to, if anything was changed.
	Cancelled(Option<EffectsSettings>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectsDialog {
	/// What the window opened with.
	initial: EffectsSettings,
	reverb: CheckboxState,
	reverb_level: SliderState,
	chorus: CheckboxState,
	chorus_level: SliderState,
	ok: ButtonState,
	cancel: ButtonState,
}

fn level_slider(level: u8) -> SliderState {
	SliderState::new(0, i64::from(MAX_LEVEL))
		.with_value(i64::from(level))
		.with_step(LEVEL_STEP)
}

fn level_of(slider: &SliderState) -> u8 {
	u8::try_from(slider.value()).unwrap_or(MAX_LEVEL)
}

impl EffectsDialog {
	pub fn new(settings: EffectsSettings) -> Self {
		Self {
			initial: settings,
			reverb: CheckboxState::with_checked(settings.reverb),
			reverb_level: level_slider(settings.reverb_level),
			chorus: CheckboxState::with_checked(settings.chorus),
			chorus_level: level_slider(settings.chorus_level),
			ok: ButtonState::new(),
			cancel: ButtonState::new(),
		}
	}

	/// The effects as the controls of the window have them now.
	pub fn settings(&self) -> EffectsSettings {
		EffectsSettings {
			reverb: self.reverb.is_checked(),
			chorus: self.chorus.is_checked(),
			reverb_level: level_of(&self.reverb_level),
			chorus_level: level_of(&self.chorus_level),
		}
	}

	/// The window as a modal layer; built anew for every event and every frame.
	pub fn ui<'a>(&'a mut self, locale: &str) -> Ui<'a, EffectsMsg> {
		let Self {
			reverb,
			reverb_level,
			chorus,
			chorus_level,
			ok,
			cancel,
			..
		} = self;
		let plain = Style::new().fg(TEXT).bg(BACKGROUND);
		let focused = Style::new().underlined();

		// One row per effect: the switch with the name, the slider of the send level, the level itself.
		let effect_row = |name: String, switch: &'a mut CheckboxState, slider: &'a mut SliderState| {
			let level = format!("{:>width$}", slider.value(), width = usize::from(VALUE_CELLS));
			let track = Slider::new()
				.style(plain)
				.filled_style(Style::new().fg(VALUE))
				.unfilled_style(Style::new().fg(LABEL))
				.focus_style(focused);
			Ui::split(
				Layout::horizontal([Constraint::Length(NAME_CELLS), Constraint::Fill(1), Constraint::Length(VALUE_CELLS)]),
				vec![
					Ui::stateful(
						Checkbox::new(name).style(plain).focus_style(focused).pressed_style(SELECTED),
						switch,
					)
					.on_toggle(|_| EffectsMsg::Changed),
					Ui::stateful(track, slider).on_value(|_| EffectsMsg::Changed),
					Ui::widget(Paragraph::new(Line::styled(level, plain))),
				],
			)
		};
		let reverb_row = effect_row(t!("dialog.effects_reverb", locale = locale).into_owned(), reverb, reverb_level);
		let chorus_row = effect_row(t!("dialog.effects_chorus", locale = locale).into_owned(), chorus, chorus_level);

		let ok_label = format!("[{}]", t!("dialog.ok", locale = locale));
		let cancel_label = format!("[{}]", t!("main.cancel", locale = locale));
		let width_of = |text: &str| u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
		let button = |text: String| Button::new(text).style(plain).focus_style(focused).pressed_style(SELECTED);
		let button_row = Ui::split(
			Layout::horizontal([
				Constraint::Fill(1),
				Constraint::Length(width_of(&ok_label)),
				Constraint::Length(1),
				Constraint::Length(width_of(&cancel_label)),
			]),
			vec![
				Ui::widget(Paragraph::new(Line::styled(
					t!("dialog.effects_hint", locale = locale).into_owned(),
					Style::new().fg(HINT),
				))),
				Ui::stateful(button(ok_label), ok).on_activate(EffectsMsg::Accept),
				Ui::widget(Paragraph::new("")),
				Ui::stateful(button(cancel_label), cancel).on_activate(EffectsMsg::Cancel),
			],
		);

		let title = format!(" {} ", t!("dialog.effects_title", locale = locale));
		let content = Ui::group(
			Block::bordered().title(title).border_style(Style::new().fg(TEXT)),
			Layout::vertical([
				Constraint::Length(1),
				Constraint::Length(1),
				Constraint::Length(1),
				Constraint::Length(1),
			]),
			vec![reverb_row, chorus_row, Ui::widget(Paragraph::new("")), button_row],
		);
		let layer = Modal::centered(WIDTH, HEIGHT)
			.style(plain)
			.content(content)
			.on_close(EffectsMsg::Cancel);
		Ui::split(Layout::vertical([Constraint::Fill(1)]), Vec::new()).modal(layer)
	}

	/// Leaving the window without `[OK]`: what to go back to, if anything was changed.
	pub fn cancel(&self) -> EffectsOutcome {
		EffectsOutcome::Cancelled((self.settings() != self.initial).then_some(self.initial))
	}

	/// What the controls reported for one input event.
	pub fn settle(&self, routed: Routed<EffectsMsg>) -> EffectsOutcome {
		let mut outcome = EffectsOutcome::Pending;
		for message in routed {
			outcome = match message {
				EffectsMsg::Changed => EffectsOutcome::Changed(self.settings()),
				EffectsMsg::Accept => EffectsOutcome::Accepted(self.settings()),
				EffectsMsg::Cancel => self.cancel(),
			};
		}
		outcome
	}
}

#[cfg(test)]
mod tests;
