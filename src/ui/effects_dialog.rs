//! F6 effects dialog (spec v1.1 §4.2): two rows — reverb and chorus — each with an on/off switch and a send
//! level. Pure state machine; drawing lives in `modals.rs`. Changes are applied live through `Changed`,
//! Enter keeps them (`Accepted`), Esc rolls back to the state the dialog opened with.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::session::EffectsSettings;

/// ← / → move a send level by this much (CC 91 / CC 93 range 0..=127).
pub const LEVEL_STEP: i16 = 5;
pub const MAX_LEVEL: u8 = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectRow {
	Reverb,
	Chorus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectsOutcome {
	Pending,
	/// Apply now, keep the dialog open.
	Changed(EffectsSettings),
	/// Enter: apply and save.
	Accepted(EffectsSettings),
	/// Esc: the state to restore, if anything was changed meanwhile.
	Cancelled(Option<EffectsSettings>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectsDialog {
	initial: EffectsSettings,
	current: EffectsSettings,
	row: EffectRow,
}

impl EffectsDialog {
	pub fn new(settings: EffectsSettings) -> Self {
		Self {
			initial: settings,
			current: settings,
			row: EffectRow::Reverb,
		}
	}

	pub fn settings(&self) -> EffectsSettings {
		self.current
	}

	pub fn row(&self) -> EffectRow {
		self.row
	}

	pub fn handle_key(&mut self, key: KeyEvent) -> EffectsOutcome {
		match key.code {
			KeyCode::Esc => {
				let rollback = (self.current != self.initial).then_some(self.initial);
				EffectsOutcome::Cancelled(rollback)
			}
			KeyCode::Enter => EffectsOutcome::Accepted(self.current),
			KeyCode::Up | KeyCode::Down => {
				self.row = match self.row {
					EffectRow::Reverb => EffectRow::Chorus,
					EffectRow::Chorus => EffectRow::Reverb,
				};
				EffectsOutcome::Pending
			}
			KeyCode::Char(' ') => {
				match self.row {
					EffectRow::Reverb => self.current.reverb = !self.current.reverb,
					EffectRow::Chorus => self.current.chorus = !self.current.chorus,
				}
				EffectsOutcome::Changed(self.current)
			}
			KeyCode::Left => self.adjust_level(-LEVEL_STEP),
			KeyCode::Right => self.adjust_level(LEVEL_STEP),
			_ => EffectsOutcome::Pending,
		}
	}

	fn adjust_level(&mut self, delta: i16) -> EffectsOutcome {
		let level = match self.row {
			EffectRow::Reverb => &mut self.current.reverb_level,
			EffectRow::Chorus => &mut self.current.chorus_level,
		};
		let adjusted = (i16::from(*level) + delta).clamp(0, i16::from(MAX_LEVEL)) as u8;
		if adjusted == *level {
			return EffectsOutcome::Pending;
		}
		*level = adjusted;
		EffectsOutcome::Changed(self.current)
	}
}

#[cfg(test)]
mod tests {
	use ratatui::crossterm::event::KeyModifiers;

	use super::*;

	fn press(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	fn settings() -> EffectsSettings {
		EffectsSettings {
			reverb: true,
			chorus: false,
			reverb_level: 40,
			chorus_level: 20,
		}
	}

	#[test]
	fn space_toggles_the_highlighted_effect_and_arrows_switch_rows() {
		let mut dialog = EffectsDialog::new(settings());
		assert_eq!(dialog.row(), EffectRow::Reverb);
		let outcome = dialog.handle_key(press(KeyCode::Char(' ')));
		assert_eq!(
			outcome,
			EffectsOutcome::Changed(EffectsSettings {
				reverb: false,
				..settings()
			})
		);
		assert_eq!(dialog.handle_key(press(KeyCode::Down)), EffectsOutcome::Pending);
		assert_eq!(dialog.row(), EffectRow::Chorus);
		dialog.handle_key(press(KeyCode::Char(' ')));
		assert!(dialog.settings().chorus);
		dialog.handle_key(press(KeyCode::Up));
		assert_eq!(dialog.row(), EffectRow::Reverb);
	}

	#[test]
	fn levels_move_in_steps_of_five_and_clamp_without_reporting_a_change() {
		let mut dialog = EffectsDialog::new(settings());
		assert_eq!(
			dialog.handle_key(press(KeyCode::Right)),
			EffectsOutcome::Changed(EffectsSettings {
				reverb_level: 45,
				..settings()
			})
		);
		for _ in 0..30 {
			dialog.handle_key(press(KeyCode::Right));
		}
		assert_eq!(dialog.settings().reverb_level, MAX_LEVEL);
		assert_eq!(
			dialog.handle_key(press(KeyCode::Right)),
			EffectsOutcome::Pending,
			"already at the top"
		);
		dialog.handle_key(press(KeyCode::Down));
		for _ in 0..10 {
			dialog.handle_key(press(KeyCode::Left));
		}
		assert_eq!(dialog.settings().chorus_level, 0);
		assert_eq!(dialog.handle_key(press(KeyCode::Left)), EffectsOutcome::Pending);
	}

	#[test]
	fn enter_accepts_and_esc_rolls_back_only_after_a_change() {
		let mut untouched = EffectsDialog::new(settings());
		assert_eq!(untouched.handle_key(press(KeyCode::Esc)), EffectsOutcome::Cancelled(None));
		let mut dialog = EffectsDialog::new(settings());
		dialog.handle_key(press(KeyCode::Left));
		assert_eq!(dialog.handle_key(press(KeyCode::Esc)), EffectsOutcome::Cancelled(Some(settings())));
		dialog.handle_key(press(KeyCode::Char(' ')));
		assert_eq!(
			dialog.handle_key(press(KeyCode::Enter)),
			EffectsOutcome::Accepted(EffectsSettings {
				reverb: false,
				reverb_level: 35,
				..settings()
			})
		);
		assert_eq!(
			dialog.handle_key(press(KeyCode::Tab)),
			EffectsOutcome::Pending,
			"unknown keys are ignored"
		);
	}
}
