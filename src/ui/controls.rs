//! The controls of the main screen (spec v1.2 §3, F-25; v1.3, F-26.1): the `[buttons]` and the
//! dropdowns — the two devices and the format of takes — a click, or Tab and Enter.
//!
//! The screen says where each control goes while it draws a frame; the controls are drawn when the
//! frame is finished, by the library's `Button` and `ComboBox` over their states. Input goes through
//! the library's tree, which keeps the focus ring and the mouse capture. A control that is not placed
//! in a frame, or is shown unavailable, takes neither the focus nor a click. The list of an open
//! dropdown is a layer of the library: it is drawn over the frame and takes all input while it is open.

use gterm::app::DrawCtx;
use gterm::buffer::ScreenBuffer;
use gterm::cell::Color;
use gterm_widgets::widgets::{Button, ButtonState, ComboBox, ComboBoxState};
use gterm_widgets::{Constraint, KeyMsg, Layout, MouseMsg, NodeState, Rect, Routed, StatefulWidget, Style, Ui};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::app::{ButtonId, Dropdown};
use super::chord::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::paint::{BACKGROUND, DISABLED, ERROR, SELECTED, TEXT, VALUE};

/// Rows of an open dropdown list, without its frame; fewer when there is less to choose from.
const LIST_ROWS: u16 = 8;

/// How a button looks (spec v1.2 §3). Under the keyboard focus any of them is underlined; while the
/// mouse holds it down, it is inverted on light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
	Normal,
	/// The selected one of several: inverted on amber.
	Active,
	/// Reacts, and what it does cannot be undone (cancelling a take): red.
	Danger,
	/// Not available now: dark, does not react.
	Disabled,
}

impl Look {
	fn style(self) -> Style {
		match self {
			Self::Normal => Style::new().fg(TEXT),
			Self::Active => Style::new().fg(BACKGROUND).bg(VALUE).bold(),
			Self::Danger => Style::new().fg(ERROR),
			Self::Disabled => Style::new().fg(DISABLED),
		}
	}

	fn reacts(self) -> bool {
		matches!(self, Self::Normal | Self::Active | Self::Danger)
	}
}

/// What an input event did to the controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
	/// The event was not for a control.
	Missed,
	/// The focus moved, a button went down or up, a list was walked or closed: the picture changed,
	/// nothing was pressed or chosen.
	Changed,
	/// A button was pressed: by Enter under the focus, or by a click released over it.
	Pressed(ButtonId),
	/// A dropdown has just dropped its list down: the list is to be filled with what there is to
	/// choose from now.
	Opened(Dropdown),
	/// A row of an open list was chosen — another one than the value in use; the list is closed.
	Chosen(Dropdown, usize),
}

/// What the controls of the tree report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
	Button(ButtonId),
	Row(Dropdown, usize),
}

/// A place in the Tab ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
	Button(ButtonId),
	Dropdown(Dropdown),
}

/// The first button after a dropdown in the order of the screen: the dropdown stands before it in the ring.
fn follower(dropdown: Dropdown) -> ButtonId {
	match dropdown {
		Dropdown::MidiIn => ButtonId::MidiFile,
		Dropdown::AudioOut => ButtonId::MasterDown,
		Dropdown::Format => ButtonId::RecordStart,
	}
}

/// The Tab ring in the order of the screen: top to bottom, left to right.
fn ring() -> Vec<Slot> {
	let mut ring = Vec::with_capacity(ButtonId::ALL.len() + Dropdown::ALL.len());
	for id in ButtonId::ALL {
		ring.extend(
			Dropdown::ALL
				.into_iter()
				.filter(|dropdown| follower(*dropdown) == id)
				.map(Slot::Dropdown),
		);
		ring.push(Slot::Button(id));
	}
	ring
}

/// Where the screen put a button in the frame being drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
	id: ButtonId,
	area: Rect,
	label: String,
	look: Look,
}

/// Where the screen put a dropdown in the frame being drawn, and what its closed field says.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
	area: Rect,
	/// The value in use; `None` when there is none, and then `absent` says so.
	current: Option<String>,
	absent: String,
	/// The value cannot be changed now (the format of a running take): shown dark, does not react.
	is_locked: bool,
}

/// One dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DropList {
	state: ComboBoxState,
	/// The rows of the list: what there was to choose from when it was last opened.
	items: Vec<String>,
	field: Option<Field>,
}

impl DropList {
	fn new() -> Self {
		let mut state = ComboBoxState::new();
		state.enabled = false;
		Self {
			state,
			items: Vec::new(),
			field: None,
		}
	}

	/// The row of the value in use, when the list has it.
	fn current_row(&self) -> Option<usize> {
		let current = self.field.as_ref()?.current.as_ref()?;
		self.items.iter().position(|item| item == current)
	}
}

/// The dropdown as the library draws it: the closed field, and the list that takes its style.
fn field_widget<'a>(rows: &'a [&'a str], text: Color) -> ComboBox<'a> {
	ComboBox::new(rows)
		.rows(LIST_ROWS)
		.style(Style::new().fg(text).bg(BACKGROUND))
		.focus_style(Style::new().underlined())
		.highlight_style(SELECTED)
}

pub struct Controls {
	ring: Vec<Slot>,
	/// One state per button, in the order of `ButtonId::ALL`.
	buttons: Vec<ButtonState>,
	placed: Vec<Placed>,
	/// The dropdowns, in the order of `Dropdown::ALL`.
	dropdowns: [DropList; 3],
}

impl Default for Controls {
	fn default() -> Self {
		Self::new()
	}
}

impl Controls {
	/// Nothing reacts until a frame has placed the controls.
	pub fn new() -> Self {
		Self {
			ring: ring(),
			buttons: ButtonId::ALL.iter().map(|_| ButtonState::disabled()).collect(),
			placed: Vec::new(),
			dropdowns: [DropList::new(), DropList::new(), DropList::new()],
		}
	}

	/// Starts a frame: the screen places its controls anew.
	pub fn begin_frame(&mut self) {
		self.placed.clear();
		for list in &mut self.dropdowns {
			list.field = None;
		}
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

	/// Reserves `width` cells at `(x, y)` for the field of a dropdown; returns the width taken. The
	/// field shows the value in use — `current` — or, when there is none, what `absent` says.
	pub fn place_dropdown(&mut self, which: Dropdown, x: u16, y: u16, width: u16, current: Option<&str>, absent: &str) -> u16 {
		self.dropdowns[which as usize].field = Some(Field {
			area: Rect::new(x, y, width, 1),
			current: current.map(str::to_owned),
			absent: absent.to_owned(),
			is_locked: false,
		});
		width
	}

	/// Marks a dropdown placed in this frame as one whose value cannot be changed now: its field
	/// still shows the value, dark, and neither opens nor takes the focus.
	pub fn lock_dropdown(&mut self, which: Dropdown) {
		if let Some(field) = &mut self.dropdowns[which as usize].field {
			field.is_locked = true;
		}
	}

	/// Ends a frame: what was placed becomes what reacts, the focus is kept on a control that does, and
	/// the controls are drawn.
	pub fn finish_frame(&mut self, buf: &mut ScreenBuffer) {
		for state in &mut self.buttons {
			state.enabled = false;
			state.node.area = Rect::default();
		}
		for placed in &self.placed {
			let state = &mut self.buttons[placed.id as usize];
			state.enabled = placed.look.reacts();
			state.node.area = placed.area;
		}
		for list in &mut self.dropdowns {
			let current_row = list.current_row();
			match &list.field {
				Some(field) => {
					list.state.enabled = !field.is_locked;
					list.state.node.area = field.area;
					list.state.selected = current_row;
					// A value that got locked cannot keep its list open.
					if field.is_locked {
						list.state.close_list();
					}
				}
				// A dropdown that left the screen cannot keep its list open.
				None => {
					list.state.enabled = false;
					list.state.node.area = Rect::default();
					list.state.close_list();
				}
			}
		}
		self.settle_focus();
		for placed in &self.placed {
			Button::new(placed.label.as_str())
				.style(placed.look.style())
				.focus_style(Style::new().underlined())
				.pressed_style(SELECTED)
				.render(placed.area, buf, &mut self.buttons[placed.id as usize]);
		}
		for list in &mut self.dropdowns {
			let DropList { state, items, field } = list;
			let Some(field) = field else {
				continue;
			};
			let rows: Vec<&str> = items.iter().map(String::as_str).collect();
			// No device is a problem of the setting: its line says so in red.
			let (shown, text) = match &field.current {
				Some(current) if field.is_locked => (current.as_str(), DISABLED),
				Some(current) => (current.as_str(), TEXT),
				None => (field.absent.as_str(), ERROR),
			};
			field_widget(&rows, text).placeholder(shown).render(field.area, buf, state);
		}
	}

	fn node(&self, slot: Slot) -> &NodeState {
		match slot {
			Slot::Button(id) => &self.buttons[id as usize].node,
			Slot::Dropdown(which) => &self.dropdowns[which as usize].state.node,
		}
	}

	fn node_mut(&mut self, slot: Slot) -> &mut NodeState {
		match slot {
			Slot::Button(id) => &mut self.buttons[id as usize].node,
			Slot::Dropdown(which) => &mut self.dropdowns[which as usize].state.node,
		}
	}

	fn reacts(&self, slot: Slot) -> bool {
		match slot {
			Slot::Button(id) => self.buttons[id as usize].enabled,
			Slot::Dropdown(which) => self.dropdowns[which as usize].state.enabled,
		}
	}

	fn focus(&mut self, target: Option<usize>) {
		for index in 0..self.ring.len() {
			let slot = self.ring[index];
			self.node_mut(slot).focused = Some(index) == target;
		}
	}

	/// The focus stays where it is while that control reacts; from a control that stopped reacting it
	/// goes to the next one that does (after `[Start]` of a take that is `[Stop]`), and at the very
	/// start to the first.
	fn settle_focus(&mut self) {
		let count = self.ring.len();
		let reacting_from = |start: usize| {
			(0..count)
				.map(|step| (start + step) % count)
				.find(|&index| self.reacts(self.ring[index]))
		};
		let target = match self.ring.iter().position(|slot| self.node(*slot).focused) {
			Some(focused) => reacting_from(focused),
			None => reacting_from(0),
		};
		self.focus(target);
	}

	/// The button under the keyboard focus.
	pub fn focused(&self) -> Option<ButtonId> {
		self.find(|state| state.node.focused)
	}

	/// The dropdown under the keyboard focus.
	pub fn focused_dropdown(&self) -> Option<Dropdown> {
		Dropdown::ALL
			.into_iter()
			.find(|which| self.dropdowns[*which as usize].state.node.focused)
	}

	/// The button the mouse holds down.
	pub fn held(&self) -> Option<ButtonId> {
		self.find(|state| state.pressed)
	}

	fn find(&self, is_wanted: impl Fn(&ButtonState) -> bool) -> Option<ButtonId> {
		self.buttons.iter().position(is_wanted).map(|index| ButtonId::ALL[index])
	}

	/// The dropdown whose list is open.
	pub fn open_dropdown(&self) -> Option<Dropdown> {
		Dropdown::ALL.into_iter().find(|which| self.dropdowns[*which as usize].state.open)
	}

	/// Drops the list of a dropdown down the way its key does (F2, F3): the focus goes to the
	/// dropdown too. `false` when the dropdown is not on the screen — then nothing happens.
	pub fn open(&mut self, which: Dropdown) -> bool {
		if !self.dropdowns[which as usize].state.enabled {
			return false;
		}
		self.close_dropdowns();
		let place = self.ring.iter().position(|slot| *slot == Slot::Dropdown(which));
		self.focus(place);
		self.dropdowns[which as usize].state.open_list();
		true
	}

	pub fn close_dropdowns(&mut self) {
		for list in &mut self.dropdowns {
			list.state.close_list();
		}
	}

	/// Fills the list of a dropdown: what there is to choose from now. The row of the value in use
	/// is the one the list starts on.
	pub fn set_items(&mut self, which: Dropdown, items: Vec<String>) {
		let list = &mut self.dropdowns[which as usize];
		list.items = items;
		let current_row = list.current_row();
		list.state.selected = current_row;
		list.state.list.select(current_row);
	}

	/// Tab and Shift+Tab walk the ring, Enter presses the button under the focus or drops the list of
	/// the dropdown under it. While a list is open, the arrows, PgUp, PgDn, Home and End walk it,
	/// Enter chooses and Esc closes. Any other key, a chord with Ctrl or Alt and a release are not for
	/// the controls.
	pub fn handle_key(&mut self, key: &KeyEvent) -> Touch {
		if key.kind == KeyEventKind::Release || key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
			return Touch::Missed;
		}
		let is_list_open = self.open_dropdown().is_some();
		let (named, modifiers) = match key.code {
			KeyCode::Tab => (NamedKey::Tab, ModifiersState::empty()),
			KeyCode::BackTab => (NamedKey::Tab, ModifiersState::SHIFT),
			KeyCode::Enter => (NamedKey::Enter, ModifiersState::empty()),
			KeyCode::Esc if is_list_open => (NamedKey::Escape, ModifiersState::empty()),
			KeyCode::Up if is_list_open => (NamedKey::ArrowUp, ModifiersState::empty()),
			KeyCode::Down if is_list_open => (NamedKey::ArrowDown, ModifiersState::empty()),
			KeyCode::PageUp if is_list_open => (NamedKey::PageUp, ModifiersState::empty()),
			KeyCode::PageDown if is_list_open => (NamedKey::PageDown, ModifiersState::empty()),
			KeyCode::Home if is_list_open => (NamedKey::Home, ModifiersState::empty()),
			KeyCode::End if is_list_open => (NamedKey::End, ModifiersState::empty()),
			_ => return Touch::Missed,
		};
		let key = Key::Named(named);
		let message = KeyMsg {
			key: &key,
			text: None,
			modifiers,
		};
		self.route(|mut tree| tree.route_key(&message))
	}

	/// A mouse event in cells of the grid; `screen` is the whole grid — an open list is placed by it.
	pub fn handle_mouse(&mut self, message: &MouseMsg, screen: Rect) -> Touch {
		self.route(|mut tree| tree.route_mouse(message, screen))
	}

	/// Sends one input event through the tree and tells what it did.
	fn route(&mut self, send: impl FnOnce(Ui<'_, Hit>) -> Routed<Hit>) -> Touch {
		let was_open = self.open_dropdown();
		let routed = self.with_tree(send);
		let was_handled = routed.handled.is_yes();
		match routed.messages.first() {
			Some(Hit::Button(id)) => Touch::Pressed(*id),
			Some(Hit::Row(which, row)) => Touch::Chosen(*which, *row),
			None => match (was_open, self.open_dropdown()) {
				(None, Some(which)) => Touch::Opened(which),
				_ if was_handled => Touch::Changed,
				_ => Touch::Missed,
			},
		}
	}

	/// Draws the list of the open dropdown over the frame; nothing while none is open.
	pub fn draw_layer(&mut self, ctx: &mut DrawCtx<'_>) {
		if self.open_dropdown().is_some() {
			self.with_tree(|tree| tree.draw_modal(ctx));
		}
	}

	/// The list of the open dropdown as it is drawn: where it sits on a grid of `screen`, and its cells.
	#[cfg(test)]
	pub fn layer(&mut self, screen: Rect) -> Option<(Rect, ScreenBuffer)> {
		self.with_tree(|tree| tree.modal_buffer(screen))
	}

	/// Builds the controls as the library's tree, in Tab order, and hands it over. The tree serves
	/// routing and the layer of an open list: the areas are those of the last frame, kept in the states.
	fn with_tree<R>(&mut self, use_tree: impl FnOnce(Ui<'_, Hit>) -> R) -> R {
		let Self {
			ring, buttons, dropdowns, ..
		} = self;
		let [midi, audio, format] = dropdowns;
		let midi_rows: Vec<&str> = midi.items.iter().map(String::as_str).collect();
		let audio_rows: Vec<&str> = audio.items.iter().map(String::as_str).collect();
		let format_rows: Vec<&str> = format.items.iter().map(String::as_str).collect();
		let mut nodes: Vec<Ui<'_, Hit>> = buttons
			.iter_mut()
			.zip(ButtonId::ALL)
			.map(|(state, id)| Ui::stateful(Button::default(), state).on_activate(Hit::Button(id)))
			.collect();
		// In the order of the ring: an earlier dropdown is in place before the next one is counted.
		for (which, state, rows) in [
			(Dropdown::MidiIn, &mut midi.state, &midi_rows),
			(Dropdown::AudioOut, &mut audio.state, &audio_rows),
			(Dropdown::Format, &mut format.state, &format_rows),
		] {
			let place = ring.iter().position(|slot| *slot == Slot::Dropdown(which)).unwrap_or(nodes.len());
			let field = Ui::combo_box(field_widget(rows, TEXT), state, move |row| Hit::Row(which, row));
			nodes.insert(place, field);
		}
		use_tree(Ui::split(Layout::vertical([Constraint::Fill(1)]), nodes))
	}
}

#[cfg(test)]
mod tests;
