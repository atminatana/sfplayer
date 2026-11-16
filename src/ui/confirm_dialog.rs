//! The confirmation window (spec v1.3, F-26.5): a question and two buttons — the action and the
//! refusal. The focus starts on the refusal, so a stray Enter changes nothing; Esc and a click outside
//! the window refuse too.
//!
//! Built from gterm-widgets controls: the Tab ring and the mouse come from the library. What the
//! answer means is decided by `App::answer`; this module only asks.

use gterm_widgets::widgets::{Block, Button, ButtonState, Paragraph};
use gterm_widgets::{Constraint, Layout, Line, Modal, Routed, Style, Ui};
use rust_i18n::t;

use super::app::Question;
use super::paint::{BACKGROUND, ERROR, SELECTED, TEXT};

/// Size of the window in cells; it shrinks to the grid when the window is smaller.
pub const WIDTH: u16 = 60;
pub const HEIGHT: u16 = 6;
/// Cells between the two buttons.
const BUTTON_GAP: u16 = 3;

/// How the window words itself, by what is asked.
impl Question {
	fn title_key(self) -> &'static str {
		match self {
			Self::Quit => "confirm.quit_title",
			Self::DeleteTake => "confirm.delete_title",
		}
	}

	fn text_key(self) -> &'static str {
		match self {
			Self::Quit => "confirm.quit_text",
			Self::DeleteTake => "confirm.delete_text",
		}
	}

	fn action_key(self) -> &'static str {
		match self {
			Self::Quit => "confirm.quit_yes",
			Self::DeleteTake => "confirm.delete_yes",
		}
	}
}

/// What the buttons of the window report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmMsg {
	Yes,
	No,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
	Pending,
	Confirmed,
	Refused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmDialog {
	question: Question,
	action: ButtonState,
	refusal: ButtonState,
}

impl ConfirmDialog {
	pub fn new(question: Question) -> Self {
		let mut refusal = ButtonState::new();
		// The library gives the focus to the first control that already holds it.
		refusal.node.focused = true;
		Self {
			question,
			action: ButtonState::new(),
			refusal,
		}
	}

	pub fn question(&self) -> Question {
		self.question
	}

	/// The window as a modal layer; built anew for every event and every frame. `take` names the
	/// running take — its length and size — for the question about deleting it; the question about
	/// quitting does not use it.
	pub fn ui<'a>(&'a mut self, locale: &str, take: &str) -> Ui<'a, ConfirmMsg> {
		let Self { question, action, refusal } = self;
		let plain = Style::new().fg(TEXT).bg(BACKGROUND);
		let focused = Style::new().underlined();

		let text = t!(question.text_key(), locale = locale, take = take).into_owned();
		let question_row = Ui::widget(Paragraph::new(Line::styled(text, plain)).centered());

		let action_label = format!("[{}]", t!(question.action_key(), locale = locale));
		let refusal_label = format!("[{}]", t!("confirm.no", locale = locale));
		let width_of = |text: &str| u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
		let button = |text: String| Button::new(text).style(plain).focus_style(focused).pressed_style(SELECTED);
		let button_row = Ui::split(
			Layout::horizontal([
				Constraint::Fill(1),
				Constraint::Length(width_of(&action_label)),
				Constraint::Length(BUTTON_GAP),
				Constraint::Length(width_of(&refusal_label)),
				Constraint::Fill(1),
			]),
			vec![
				Ui::widget(Paragraph::new("")),
				Ui::stateful(button(action_label), action).on_activate(ConfirmMsg::Yes),
				Ui::widget(Paragraph::new("")),
				Ui::stateful(button(refusal_label), refusal).on_activate(ConfirmMsg::No),
				Ui::widget(Paragraph::new("")),
			],
		);

		let title = format!(" {} ", t!(question.title_key(), locale = locale));
		let content = Ui::group(
			Block::bordered().title(title).border_style(Style::new().fg(ERROR)),
			Layout::vertical([
				Constraint::Length(1),
				Constraint::Length(1),
				Constraint::Length(1),
				Constraint::Length(1),
			]),
			vec![
				Ui::widget(Paragraph::new("")),
				question_row,
				Ui::widget(Paragraph::new("")),
				button_row,
			],
		);
		let layer = Modal::centered(WIDTH, HEIGHT)
			.style(plain)
			.content(content)
			.on_close(ConfirmMsg::No);
		Ui::split(Layout::vertical([Constraint::Fill(1)]), Vec::new()).modal(layer)
	}

	/// What the controls reported for one input event; a key no control took means nothing here.
	pub fn settle(&self, routed: Routed<ConfirmMsg>) -> ConfirmOutcome {
		match routed.messages.first() {
			Some(ConfirmMsg::Yes) => ConfirmOutcome::Confirmed,
			Some(ConfirmMsg::No) => ConfirmOutcome::Refused,
			None => ConfirmOutcome::Pending,
		}
	}
}

#[cfg(test)]
mod tests;
