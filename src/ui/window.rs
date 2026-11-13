//! The gterm window (spec v1.2 §8): the UI thread is the process main thread running gterm's event loop.
//!
//! All behaviour stays in `App` (key handling) and `fulfil` (requests applied to the `Session`); this
//! module only hands window events over (keys are translated by `keymap`), drives the periodic work
//! from the tick, and redraws when what the screen shows has changed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use gterm::app::{App as GridWindow, AppHandler, AppOptions, Ctx, DrawCtx, Grid, PointerEvent};
use gterm::buffer::ScreenBuffer;
use gterm::config::{BACKEND_ENV, Config as GridConfig};
use gterm::font::{FONT_SCALE, FontFamily, glyph_px_for};
use gterm::rasterizer::Rasterizer;
use gterm_widgets::{MouseMsg, Rect, SystemClipboard};
use rust_i18n::t;
use winit::event::ElementState;

use super::app::{App, FRAME_INTERVAL, Request, fulfil};
use super::buttons::{Buttons, Touch};
use super::chord::{KeyCode, KeyEvent, KeyEventKind};
use super::dialog_screen;
use super::input_dialog::{InputDialog, InputOutcome};
use super::keymap::{Reading, WindowKey, key_from_window, quits_from_a_field, reading_in_a_dialog};
use super::main_screen::{self, BACKGROUND, MIN_COLS, MIN_ROWS, TEXT};
use super::path_input::{self, InputKind};
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
	tracing::info!(font = family.family(), font_size, "window font");
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
		buttons: Buttons::new(),
		clipboard: SystemClipboard::new(),
		input: None,
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

/// Whether the mouse event is over a cell of `area`.
fn is_inside(area: Rect, message: &MouseMsg) -> bool {
	match (u16::try_from(message.col), u16::try_from(message.row)) {
		(Ok(col), Ok(row)) => area.contains(col, row),
		_ => false,
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
	/// The `[buttons]` of the main screen: where the last frame put them, the focus, the one held down.
	buttons: Buttons,
	/// Where Ctrl+V takes text from for the field of the input dialog.
	clipboard: SystemClipboard,
	/// The input dialog on widget controls, while it is open.
	input: Option<InputDialog>,
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
		main_screen::render(ctx.screen, &self.app, &state.view, &mut self.buttons);
		dialog_screen::render(ctx.screen, &self.app);
		self.shown = Some(state);
		ctx.redraw();
	}

	/// Shows the input dialog for the path `App` wants asked.
	fn adopt_path_request(&mut self) {
		if let Some(request) = self.app.path_request.take() {
			self.input = Some(InputDialog::new(
				request.kind,
				&request.initial,
				&request.history,
				path_input::list_dir,
			));
		}
	}

	/// A key press on the main screen. A command key goes to `App`; of the rest, Tab and Enter belong
	/// to the buttons, and what is left may be a note. The flag tells that the picture changed without
	/// `App` knowing (the focus moved).
	fn key_on_screen(&mut self, key: KeyEvent) -> (Option<Request>, bool) {
		if self.app.modal.is_some() || self.app.bindings.action_for(&key).is_some() {
			return (self.app.handle_key(key), false);
		}
		match self.buttons.handle_key(&key) {
			Touch::Missed => (self.app.handle_key(key), false),
			Touch::Changed => (None, true),
			Touch::Pressed(button) => (self.app.press_button(button), true),
		}
	}

	/// A key press or release for the open input dialog.
	fn key_to_input(&mut self, event: &winit::event::KeyEvent, ctx: &mut Ctx<'_>) {
		let Self { input, clipboard, app, .. } = self;
		let Some(dialog) = input else {
			return;
		};
		// The dialog sees releases too (the library tracks held keys by them); only presses act.
		let routed = dialog.ui(app.locale, clipboard).key(event, ctx);
		if event.state != ElementState::Pressed {
			return;
		}
		let outcome = dialog.settle(routed, Some(&event.logical_key), path_input::list_dir);
		self.conclude_input(outcome);
		self.refresh(ctx, true);
	}

	/// A mouse event for the open input dialog.
	fn pointer_to_input(&mut self, event: &PointerEvent<'_>, ctx: &mut Ctx<'_>) {
		let Self { input, clipboard, app, .. } = self;
		let Some(dialog) = input else {
			return;
		};
		let routed = dialog.ui(app.locale, clipboard).pointer(event, ctx);
		let outcome = dialog.settle(routed, None, path_input::list_dir);
		self.conclude_input(outcome);
		self.refresh(ctx, true);
	}

	/// Acts on what the input dialog decided. A submitted path is applied, and the dialog closes only
	/// when that worked — otherwise it stays open and shows why not.
	fn conclude_input(&mut self, outcome: InputOutcome) {
		let path = match outcome {
			InputOutcome::Pending => return,
			InputOutcome::Cancelled => {
				self.input = None;
				return;
			}
			InputOutcome::Submit(path) => path,
		};
		let Some(dialog) = &mut self.input else {
			return;
		};
		let applied = match dialog.kind() {
			InputKind::Sf2File => self.session.load_bank(&path).map_err(|error| error.to_string()),
			InputKind::RecordDir => match record_dir_from(&path) {
				Some(dir) => {
					self.session.set_record_dir(dir);
					Ok(())
				}
				None => Err(t!("dialog.not_a_folder", locale = self.app.locale).into_owned()),
			},
		};
		match applied {
			Ok(()) => {
				self.input = None;
				self.session.save_config();
			}
			Err(message) => dialog.set_error(message),
		}
	}
}

/// What a path entered in the folder dialog means: an empty one is the default folder (`Some(None)`),
/// an existing file is not a folder (`None`), anything else is the folder — the recorder creates it
/// when a take starts.
fn record_dir_from(path: &Path) -> Option<Option<PathBuf>> {
	if path.as_os_str().is_empty() {
		return Some(None);
	}
	(!path.is_file()).then(|| Some(path.to_owned()))
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
		let command = key_from_window(&window_key, Reading::Command);
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
		let Some(command) = command else {
			return;
		};
		// Hotkeys and notes go by the place of the key; what is typed into a dialog goes by the layout.
		let key = if self.app.modal.is_some() && reading_in_a_dialog(&command) == Reading::Text {
			key_from_window(&window_key, Reading::Text).unwrap_or(command)
		} else {
			command
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
		let force = self.app.take_dirty() || has_changed;
		self.refresh(ctx, force);
	}

	/// Text that arrives outside key presses (input methods) belongs to the field of the input dialog.
	fn on_text(&mut self, text: &str, ctx: &mut Ctx<'_>) {
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
		if self.input.is_some() {
			self.pointer_to_input(event, ctx);
			return;
		}
		let message = MouseMsg::from_pointer(event, ctx);
		let request = if self.app.modal.is_some() {
			// A dialog takes the mouse from the screen under it; a click outside closes it like Esc.
			let dialog = dialog_screen::area(&self.app, Rect::of_screen(ctx.screen));
			if !message.is_left_down() || dialog.is_some_and(|area| is_inside(area, &message)) {
				return;
			}
			self.app.dismiss_modal()
		} else {
			match self.buttons.handle_mouse(&message) {
				Touch::Missed => return,
				Touch::Changed => None,
				Touch::Pressed(button) => self.app.press_button(button),
			}
		};
		self.apply(request);
		self.adopt_path_request();
		self.refresh(ctx, true);
	}

	/// The input dialog is a modal layer of the widget library: it is drawn over the frame, not into it.
	fn on_redraw(&mut self, ctx: &mut DrawCtx<'_>) {
		let Self { input, clipboard, app, .. } = self;
		if let Some(dialog) = input {
			dialog.ui(app.locale, clipboard).draw_modal(ctx);
		}
	}

	fn on_focus(&mut self, focused: bool, ctx: &mut Ctx<'_>) {
		// Releases that happen outside the window never arrive: without this a held note would hang.
		if !focused {
			let events = self.app.keyboard.all_notes_off();
			if !events.is_empty() {
				self.apply(Some(Request::Notes(events)));
			}
			self.refresh(ctx, false);
		}
	}

	fn on_tick(&mut self, ctx: &mut Ctx<'_>) {
		self.session.poll();
		self.app.recording = self.session.recording_elapsed();
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
