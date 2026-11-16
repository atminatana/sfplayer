//! The gterm window (spec v1.2 §8): the UI thread is the process main thread running gterm's event loop.
//!
//! All behaviour stays in `App` (key handling) and `fulfil` (requests applied to the `Session`); this
//! module only hands window events over (keys are translated by `keymap`), drives the periodic work
//! from the tick, and redraws when what the screen shows has changed. Handing events over to the
//! windows built on widget controls is in the submodule `layers`.

mod layers;

use std::time::{Duration, Instant};

use anyhow::Context;
use gterm::app::{App as GridWindow, AppHandler, AppOptions, Ctx, DrawCtx, Grid, PointerEvent};
use gterm::buffer::ScreenBuffer;
use gterm::config::{BACKEND_ENV, Config as GridConfig};
use gterm::font::{FONT_SCALE, FontFamily, glyph_px_for};
use gterm::rasterizer::Rasterizer;
use gterm_widgets::{MouseMsg, Rect, SystemClipboard};

use super::app::{App, Dropdown, FRAME_INTERVAL, KEYBOARD_CHANNEL, Request};
use super::chord::{KeyCode, KeyEvent, KeyEventKind};
use super::confirm_dialog::ConfirmDialog;
use super::controls::{Controls, Touch};
use super::devices::DeviceRows;
use super::effects_dialog::EffectsDialog;
use super::fulfil::fulfil;
use super::help_dialog::HelpDialog;
use super::input_dialog::InputDialog;
use super::keymap::{WindowKey, key_from_window, quits_from_a_field};
use super::keys::Action;
use super::main_screen::{self, BACKGROUND, MIN_COLS, MIN_ROWS, TEXT};
use super::path_input;
use super::piano::Piano;
use super::preset_dialog::PresetDialog;
use super::view::View;
use crate::session::{Notice, Session};

const WINDOW_TITLE: &str = "sf2_player";
/// Logical font size when the config does not set one (spec v1.2 §7).
pub const DEFAULT_FONT_SIZE_PX: u16 = 14;
/// Half of the blink period of the recording dot: a new frame is due whenever this phase flips.
const BLINK_HALF_PERIOD_MS: u128 = 500;
/// What the user may choose about the window itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowOptions {
	/// Font family; `None` = the system monospace font.
	pub font: Option<String>,
	/// Logical font size in pixels; `None` = [`DEFAULT_FONT_SIZE_PX`].
	pub font_size: Option<u16>,
	/// Rendering backend (`auto`, `gl`, `gles`, `soft`); `None` = the `TERM_BACKEND` variable, else automatic.
	pub backend: Option<String>,
}

/// Opens the window and runs the UI until the user quits or closes it. The take in progress is finished
/// and the config saved when the window goes away, whichever way that happens.
pub fn run(app: App, mut session: Session, options: &WindowOptions) -> anyhow::Result<()> {
	let backend_argument = options.backend.as_ref().map(|backend| format!("--backend={backend}"));
	let backend_from_environment = std::env::var(BACKEND_ENV).ok();
	let grid_config =
		GridConfig::parse(backend_argument.iter(), backend_from_environment.as_deref()).context("invalid rendering backend")?;

	// Choosing a system font reads every installed font; how long that took goes to the log.
	let font_search = Instant::now();
	let family = match options.font.as_deref().filter(|name| !name.is_empty()) {
		Some(name) => match FontFamily::system(name) {
			Ok(family) => family,
			Err(error) => {
				tracing::warn!(font = name, %error, "configured font is unavailable, using the system monospace font");
				session.notice = Some(Notice::FontFallback(name.to_owned()));
				FontFamily::system_monospace().context("no monospace font found in the system")?
			}
		},
		None => FontFamily::system_monospace().context("no monospace font found in the system")?,
	};
	let font_size = options.font_size.unwrap_or(DEFAULT_FONT_SIZE_PX);
	let search_ms = font_search.elapsed().as_millis();
	tracing::info!(font = family.family(), font_size, search_ms, "window font");
	let rasterizer = Rasterizer::new_at(family, glyph_px_for(font_size, FONT_SCALE, 1.0)).context("cannot prepare the window font")?;

	let screen = ScreenBuffer::with_default_colors(usize::from(MIN_COLS), usize::from(MIN_ROWS), TEXT, BACKGROUND);
	let window_options = AppOptions {
		font_size_px: font_size,
		tick_period: Some(FRAME_INTERVAL),
		..AppOptions::default()
	};
	let window = Window {
		app,
		session,
		shown: None,
		controls: Controls::new(),
		device_rows: Dropdown::ALL.map(|_| DeviceRows::default()),
		piano: Piano::new(KEYBOARD_CHANNEL),
		clipboard: SystemClipboard::new(),
		input: None,
		confirm: None,
		preset: None,
		effects: None,
		help: None,
	};
	GridWindow::new(WINDOW_TITLE, screen, rasterizer, grid_config, window_options)
		.run(window)
		// The window error holds platform handles that cannot cross threads; its text is all the caller needs.
		.map_err(|error| anyhow::anyhow!("window: {error}"))
}

/// Everything the screen depends on. Two equal values draw the same frame, so the window is redrawn
/// only when this changes — an idle program draws nothing (spec v1.2 N-13).
#[derive(Debug, Clone, PartialEq)]
struct FrameState {
	view: View,
	clipping: bool,
	/// Whole seconds of the take and the phase of its blinking dot; `None` when not recording.
	recording: Option<(u64, bool)>,
	octave: u8,
	velocity: u8,
	grid: (usize, usize),
}

impl FrameState {
	fn new(view: View, app: &App, grid: (usize, usize)) -> Self {
		Self {
			view,
			clipping: app.clipping,
			recording: app.recording.map(|elapsed| (elapsed.as_secs(), blink_phase(elapsed))),
			octave: app.keyboard.octave(),
			velocity: app.keyboard.velocity(),
			grid,
		}
	}
}

fn blink_phase(elapsed: Duration) -> bool {
	(elapsed.as_millis() / BLINK_HALF_PERIOD_MS).is_multiple_of(2)
}

struct Window {
	app: App,
	session: Session,
	/// What the last drawn frame showed; `None` before the first frame.
	shown: Option<FrameState>,
	/// The buttons and the device dropdowns of the main screen: where the last frame put them, the
	/// focus, the button held down, the list that is open.
	controls: Controls,
	/// What the rows of each device dropdown mean, in the order of `Dropdown::ALL`: taken from the
	/// system when the list was last opened.
	device_rows: [DeviceRows; 2],
	/// The on-screen piano: the note the mouse holds on it.
	piano: Piano,
	/// Where Ctrl+V takes text from for the field of the input dialog.
	clipboard: SystemClipboard,
	/// The input dialog on widget controls, while it is open.
	input: Option<InputDialog>,
	/// The confirmation window on widget controls, while `App` has a question open.
	confirm: Option<ConfirmDialog>,
	/// The preset window on widget controls, while it is open.
	preset: Option<PresetDialog>,
	/// The effects window on widget controls, while it is open.
	effects: Option<EffectsDialog>,
	/// The help window on widget controls, while it is open.
	help: Option<HelpDialog>,
}

impl Window {
	fn apply(&mut self, request: Option<Request>) {
		if let Some(request) = request {
			fulfil(request, &mut self.app, &mut self.session);
		}
	}

	/// Redraws when the screen content changed, or unconditionally when `force` is set.
	fn refresh(&mut self, ctx: &mut Ctx<'_>, force: bool) {
		let grid = (ctx.screen.cols(), ctx.screen.rows());
		let state = FrameState::new(View::from_session(&self.session), &self.app, grid);
		if !force && self.shown.as_ref() == Some(&state) {
			return;
		}
		main_screen::render(ctx.screen, &self.app, &state.view, &mut self.controls);
		self.shown = Some(state);
		ctx.redraw();
	}

	/// A key press on the main screen. The open list of a dropdown takes every key but the quit
	/// command. Otherwise a command key goes to `App`; of the rest, Tab and Enter belong to the
	/// controls, and what is left may be a note. The flag tells that the picture changed without
	/// `App` knowing (the focus moved, a list was walked).
	fn key_on_screen(&mut self, key: KeyEvent) -> (Option<Request>, bool) {
		let action = self.app.bindings.action_for(&key);
		if self.controls.open_dropdown().is_some() {
			if action != Some(Action::Quit) {
				let touch = self.controls.handle_key(&key);
				return (self.touched(touch), touch != Touch::Missed);
			}
			self.controls.close_dropdowns();
		}
		if action.is_some() {
			return (self.app.handle_key(key), false);
		}
		match self.controls.handle_key(&key) {
			Touch::Missed => (self.app.handle_key(key), false),
			touch => (self.touched(touch), true),
		}
	}

	/// Offers a mouse event to the on-screen piano; `true` when the piano took it. The notes go the way
	/// of the text keyboard's, and the screen follows when the audio thread reports them sounding.
	fn pointer_to_piano(&mut self, message: &MouseMsg, ctx: &mut Ctx<'_>) -> bool {
		let origin = main_screen::piano_origin(Rect::of_screen(ctx.screen));
		let keyboard = &self.app.keyboard;
		let Some(events) = self.piano.handle_mouse(message, origin, keyboard.base_key(), keyboard.velocity()) else {
			return false;
		};
		if !events.is_empty() {
			self.apply(Some(Request::Notes(events)));
		}
		true
	}
}

impl AppHandler for Window {
	fn on_backend_changed(&mut self, name: &'static str, ctx: &mut Ctx<'_>) {
		tracing::info!(backend = name, "window backend");
		self.refresh(ctx, true);
	}

	fn on_grid_resized(&mut self, _old: Grid, ctx: &mut Ctx<'_>) {
		self.refresh(ctx, true);
	}

	fn on_key(&mut self, event: &winit::event::KeyEvent, ctx: &mut Ctx<'_>) {
		let window_key = WindowKey::of(event, ctx.modifiers);
		let command = key_from_window(&window_key);
		// A held Enter must not go on to act in the dialog its first press opened.
		if command.is_some_and(|key| key.code == KeyCode::Enter && key.kind == KeyEventKind::Repeat) {
			return;
		}
		if self.input.is_some() {
			let is_quitting = command.is_some_and(|key| quits_from_a_field(&self.app.bindings, &key));
			if !is_quitting {
				self.key_to_input(event, ctx);
				return;
			}
			self.input = None;
		}
		if self.preset.is_some() {
			let is_quitting = command.is_some_and(|key| quits_from_a_field(&self.app.bindings, &key));
			if !is_quitting {
				self.key_to_preset(event, ctx);
				return;
			}
			self.preset = None;
		}
		let bindings = &self.app.bindings;
		let is_quitting = command.is_some_and(|key| key.kind == KeyEventKind::Press && bindings.action_for(&key) == Some(Action::Quit));
		// The effects window has no field to type into, so every quit key quits from it; the window is
		// left the way `[Cancel]` leaves it.
		if self.effects.is_some() {
			if !is_quitting {
				self.key_to_effects(event, ctx);
				return;
			}
			self.cancel_effects();
		}
		if self.help.is_some() {
			if !is_quitting {
				self.key_to_help(event, ctx);
				return;
			}
			self.help = None;
		}
		// The keys of an open question go to the controls of its window. The quit command goes on to
		// `App`, where asking twice means quitting.
		if self.confirm.is_some() && !is_quitting {
			self.key_to_confirm(event, ctx);
			return;
		}
		let Some(key) = command else {
			return;
		};
		// Raw key stream for diagnosing input (auto-repeat, release events); spec N-07.
		tracing::debug!(code = ?key.code, kind = ?key.kind, modifiers = ?key.modifiers, "key event");
		let (request, has_changed) = if key.kind == KeyEventKind::Release {
			(self.app.handle_release(key), false)
		} else {
			self.key_on_screen(key)
		};
		self.apply(request);
		if self.app.should_quit {
			ctx.exit();
			return;
		}
		self.adopt_path_request();
		self.adopt_preset_request();
		self.adopt_effects_request();
		self.adopt_help_request();
		self.adopt_dropdown_request();
		self.sync_confirm();
		let force = self.app.take_dirty() || has_changed;
		self.refresh(ctx, force);
	}

	/// Text that arrives outside key presses (input methods) belongs to the field of the open window:
	/// the filter of the preset window or the path of the input dialog.
	fn on_text(&mut self, text: &str, ctx: &mut Ctx<'_>) {
		if self.preset.is_some() {
			self.text_to_preset(text, ctx);
			return;
		}
		let Self { input, clipboard, app, .. } = self;
		let Some(dialog) = input else {
			return;
		};
		let routed = dialog.ui(app.locale, clipboard).text(text);
		let outcome = dialog.settle(routed, None, path_input::list_dir);
		self.conclude_input(outcome);
		self.refresh(ctx, true);
	}

	fn on_pointer(&mut self, event: &PointerEvent<'_>, ctx: &mut Ctx<'_>) {
		let message = MouseMsg::from_pointer(event, ctx);
		// A note the mouse holds on the piano is the piano's to end, even if a dialog opened meanwhile;
		// a new one starts only on the bare main screen.
		let has_dialog = self.input.is_some() || self.confirm.is_some() || self.preset.is_some();
		let has_window = has_dialog || self.effects.is_some() || self.help.is_some();
		let is_screen_bare = !has_window && self.controls.open_dropdown().is_none();
		if (is_screen_bare || self.piano.is_dragging()) && self.pointer_to_piano(&message, ctx) {
			return;
		}
		if self.input.is_some() {
			self.pointer_to_input(event, ctx);
			return;
		}
		if self.confirm.is_some() {
			self.pointer_to_confirm(event, ctx);
			return;
		}
		if self.preset.is_some() {
			self.pointer_to_preset(event, ctx);
			return;
		}
		if self.effects.is_some() {
			self.pointer_to_effects(event, ctx);
			return;
		}
		if self.help.is_some() {
			self.pointer_to_help(event, ctx);
			return;
		}
		let request = match self.controls.handle_mouse(&message, Rect::of_screen(ctx.screen)) {
			Touch::Missed => return,
			touch => self.touched(touch),
		};
		self.apply(request);
		self.adopt_path_request();
		self.adopt_preset_request();
		self.adopt_effects_request();
		self.sync_confirm();
		self.refresh(ctx, true);
	}

	/// The windows on widget controls and the open list of a dropdown are layers of the library: drawn
	/// over the frame, not into it.
	fn on_redraw(&mut self, ctx: &mut DrawCtx<'_>) {
		let take = self.take_summary();
		let Self {
			input,
			confirm,
			preset,
			effects,
			help,
			controls,
			clipboard,
			app,
			..
		} = self;
		if let Some(dialog) = input {
			dialog.ui(app.locale, clipboard).draw_modal(ctx);
		} else if let Some(dialog) = confirm {
			dialog.ui(app.locale, &take).draw_modal(ctx);
		} else if let Some(dialog) = preset {
			dialog.ui(app.locale, clipboard).draw_modal(ctx);
		} else if let Some(dialog) = effects {
			dialog.ui(app.locale).draw_modal(ctx);
		} else if let Some(dialog) = help {
			let grid_rows = u16::try_from(ctx.grid().rows).unwrap_or(u16::MAX);
			dialog.ui(app.locale, grid_rows).draw_modal(ctx);
		} else {
			controls.draw_layer(ctx);
		}
	}

	fn on_focus(&mut self, focused: bool, ctx: &mut Ctx<'_>) {
		// Releases that happen outside the window never arrive: without this a held note would hang.
		if !focused {
			let mut events = self.app.keyboard.all_notes_off();
			events.extend(self.piano.release());
			if !events.is_empty() {
				self.apply(Some(Request::Notes(events)));
			}
			self.refresh(ctx, false);
		}
	}

	fn on_tick(&mut self, ctx: &mut Ctx<'_>) {
		self.session.poll();
		self.app.set_recording(self.session.recording_elapsed());
		self.sync_confirm();
		self.app.update_meters(self.session.shared.clips(), Instant::now());
		let force = self.app.take_dirty();
		self.refresh(ctx, force);
	}
}

impl Drop for Window {
	/// gterm drops the handler when its loop ends — by `ctx.exit()`, by closing the window, or on a
	/// window error — so this is the one place that sees every way out.
	fn drop(&mut self) {
		self.session.stop_recording();
		self.session.save_config();
		tracing::info!(xruns = self.session.shared.xruns(), "sf2_player exiting");
	}
}

#[cfg(test)]
mod tests;
