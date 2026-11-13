//! The `[buttons]` of the main screen as controls (spec v1.2 §3, F-25): a click, or Tab and Enter.
//!
//! The screen says where each button goes while it draws a frame; the buttons are drawn when the frame
//! is finished, by the library's `Button` over its `ButtonState`. Input goes through the library's
//! tree, which keeps the focus ring and the mouse capture. A button that is not placed in a frame, or is
//! shown unavailable, takes neither the focus nor a click.

use gterm::buffer::ScreenBuffer;
use gterm_widgets::widgets::{Button, ButtonState};
use gterm_widgets::{Constraint, KeyMsg, Layout, MouseMsg, Rect, Routed, StatefulWidget, Style, Ui};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::app::ButtonId;
use super::chord::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::paint::{BACKGROUND, DISABLED, SELECTED, TEXT, VALUE};

/// How a button looks (spec v1.2 §3). Under the keyboard focus any of them is underlined; while the
/// mouse holds it down, it is inverted on light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
	Normal,
	/// The selected one of several: inverted on amber.
	Active,
	/// The selected one that cannot be changed now (the format of a running take): looks active, does not react.
	ActiveLocked,
	/// Not available now: dark, does not react.
	Disabled,
}

impl Look {
	fn style(self) -> Style {
		match self {
			Self::Normal => Style::new().fg(TEXT),
			Self::Active | Self::ActiveLocked => Style::new().fg(BACKGROUND).bg(VALUE).bold(),
			Self::Disabled => Style::new().fg(DISABLED),
		}
	}

	fn reacts(self) -> bool {
		matches!(self, Self::Normal | Self::Active)
	}
}

/// What an input event did to the buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
	/// The event was not for a button.
	Missed,
	/// The focus moved or a button went down or up: the picture changed, nothing was pressed.
	Changed,
	/// A button was pressed: by Enter under the focus, or by a click released over it.
	Pressed(ButtonId),
}

/// Where the screen put a button in the frame being drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
	id: ButtonId,
	area: Rect,
	label: String,
	look: Look,
}

pub struct Buttons {
	/// One state per button, in the order of `ButtonId::ALL`.
	states: Vec<ButtonState>,
	placed: Vec<Placed>,
}

impl Default for Buttons {
	fn default() -> Self {
		Self::new()
	}
}

impl Buttons {
	/// Nothing reacts until a frame has placed the buttons.
	pub fn new() -> Self {
		Self {
			states: ButtonId::ALL.iter().map(|_| ButtonState::disabled()).collect(),
			placed: Vec::new(),
		}
	}

	/// Starts a frame: the screen places its buttons anew.
	pub fn begin_frame(&mut self) {
		self.placed.clear();
	}

	/// Reserves the cells of `[label]` at `(x, y)`; returns the width taken.
	pub fn place(&mut self, id: ButtonId, x: u16, y: u16, label: &str, look: Look) -> u16 {
		let label = format!("[{label}]");
		let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
		self.placed.push(Placed {
			id,
			area: Rect::new(x, y, width, 1),
			label,
			look,
		});
		width
	}

	/// Ends a frame: what was placed becomes what reacts, the focus is kept on a button that does, and
	/// the buttons are drawn.
	pub fn finish_frame(&mut self, buf: &mut ScreenBuffer) {
		for state in &mut self.states {
			state.enabled = false;
			state.node.area = Rect::default();
		}
		for placed in &self.placed {
			let state = &mut self.states[placed.id as usize];
			state.enabled = placed.look.reacts();
			state.node.area = placed.area;
		}
		self.settle_focus();
		for placed in &self.placed {
			Button::new(placed.label.as_str())
				.style(placed.look.style())
				.focus_style(Style::new().underlined())
				.pressed_style(SELECTED)
				.render(placed.area, buf, &mut self.states[placed.id as usize]);
		}
	}

	/// The focus stays where it is while that button reacts; from a button that stopped reacting it
	/// goes to the next one that does (after `[Start]` of a take that is `[Stop]`), and at the very
	/// start to the first.
	fn settle_focus(&mut self) {
		let count = self.states.len();
		let reacting_from = |start: usize| {
			(0..count)
				.map(|step| (start + step) % count)
				.find(|&index| self.states[index].enabled)
		};
		let target = match self.states.iter().position(|state| state.node.focused) {
			Some(focused) => reacting_from(focused),
			None => reacting_from(0),
		};
		for (index, state) in self.states.iter_mut().enumerate() {
			state.node.focused = Some(index) == target;
		}
	}

	pub fn focused(&self) -> Option<ButtonId> {
		self.find(|state| state.node.focused)
	}

	/// The button the mouse holds down.
	pub fn held(&self) -> Option<ButtonId> {
		self.find(|state| state.pressed)
	}

	fn find(&self, is_wanted: impl Fn(&ButtonState) -> bool) -> Option<ButtonId> {
		self.states.iter().position(is_wanted).map(|index| ButtonId::ALL[index])
	}

	/// Tab and Shift+Tab walk the ring, Enter presses the button under the focus. Any other key, a
	/// chord with Ctrl or Alt and a release are not for the buttons.
	pub fn handle_key(&mut self, key: &KeyEvent) -> Touch {
		if key.kind == KeyEventKind::Release || key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
			return Touch::Missed;
		}
		let (named, modifiers) = match key.code {
			KeyCode::Tab => (NamedKey::Tab, ModifiersState::empty()),
			KeyCode::BackTab => (NamedKey::Tab, ModifiersState::SHIFT),
			KeyCode::Enter => (NamedKey::Enter, ModifiersState::empty()),
			_ => return Touch::Missed,
		};
		let key = Key::Named(named);
		let message = KeyMsg {
			key: &key,
			text: None,
			modifiers,
		};
		touch_of(self.tree().route_key(&message))
	}

	/// A mouse event in cells of the grid.
	pub fn handle_mouse(&mut self, message: &MouseMsg) -> Touch {
		// The area of the screen matters to a modal layer only, and this tree has none.
		touch_of(self.tree().route_mouse(message, Rect::default()))
	}

	/// The buttons as the library's tree, in Tab order. It is used for routing only: the areas are
	/// those of the last frame, kept in the states.
	fn tree(&mut self) -> Ui<'_, ButtonId> {
		let buttons = self
			.states
			.iter_mut()
			.zip(ButtonId::ALL)
			.map(|(state, id)| Ui::stateful(Button::default(), state).on_activate(id))
			.collect();
		Ui::split(Layout::vertical([Constraint::Fill(1)]), buttons)
	}
}

fn touch_of(routed: Routed<ButtonId>) -> Touch {
	let was_handled = routed.handled.is_yes();
	match routed.messages.first() {
		Some(id) => Touch::Pressed(*id),
		None if was_handled => Touch::Changed,
		None => Touch::Missed,
	}
}

#[cfg(test)]
mod tests;
