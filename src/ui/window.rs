//! The gterm window (spec v1.2 §8): the UI thread is the process main thread running gterm's event loop.
//!
//! All behaviour stays in `App` (key handling) and `fulfil` (requests applied to the `Session`); this
//! module only hands window events over (keys are translated by `keymap`), drives the periodic work
//! from the tick, and redraws when what the screen shows has changed.

use std::time::{Duration, Instant};

use anyhow::Context;
use gterm::app::{App as GridWindow, AppHandler, AppOptions, Ctx, DrawCtx, Grid, PointerEvent};
use gterm::buffer::ScreenBuffer;
use gterm::config::{BACKEND_ENV, Config as GridConfig};
use gterm::font::{FONT_SCALE, FontFamily, glyph_px_for};
use gterm::rasterizer::Rasterizer;
use gterm_widgets::{Clipboard, SystemClipboard};
use winit::event::ElementState;

use super::app::{App, FRAME_INTERVAL, Modal, Request, fulfil};
use super::chord::KeyEventKind;
use super::dialog_screen;
use super::input_dialog::{InputDialog, InputKind, InputOutcome};
use super::keymap::{Reading, WindowKey, is_paste_chord, key_from_window, quits_from_a_field, reading_in_a_dialog};
use super::main_screen::{self, BACKGROUND, MIN_COLS, MIN_ROWS, TEXT};
use super::path_input;
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

fn blink_phase(elapsed: Duration) -> bool {
	(elapsed.as_millis() / BLINK_HALF_PERIOD_MS).is_multiple_of(2)
}

struct Window {
	app: App,
	session: Session,
	/// What the last drawn frame showed; `None` before the first frame.
	shown: Option<FrameState>,
	/// Where Ctrl+V takes text from for the fields of the dialogs.
	clipboard: SystemClipboard,
	/// The input dialog on widget controls, while it is open. It stands in for the path dialog of `App`.
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
		main_screen::render(ctx.screen, &self.app, &state.view);
		dialog_screen::render(ctx.screen, &self.app);
		self.shown = Some(state);
		ctx.redraw();
	}

	/// The SoundFont path dialog is shown as the input dialog on widget controls: take over what `App` opened.
	fn adopt_path_dialog(&mut self) {
		if let Some(Modal::Sf2Path(path)) = &self.app.modal {
			self.input = Some(InputDialog::new(
				InputKind::Sf2File,
				&path.text(),
				path.recent(),
				path_input::list_dir,
			));
			self.app.modal = None;
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

	/// Acts on what the input dialog decided: a submitted bank is loaded, and the dialog closes only
	/// when that worked — otherwise it stays open and shows why not.
	fn conclude_input(&mut self, outcome: InputOutcome) {
		match outcome {
			InputOutcome::Pending => {}
			InputOutcome::Cancelled => self.input = None,
			InputOutcome::Submit(path) => match self.session.load_bank(&path) {
				Ok(()) => {
					self.input = None;
					self.session.save_config();
				}
				Err(error) => {
					if let Some(dialog) = &mut self.input {
						dialog.set_error(error.to_string());
					}
				}
			},
		}
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
		let command = key_from_window(&window_key, Reading::Command);
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
		let request = if is_paste_chord(&command) && self.app.modal.is_some() {
			if let Some(text) = self.clipboard.get() {
				self.app.handle_paste(&text);
			}
			None
		} else if key.kind == KeyEventKind::Release {
			self.app.handle_release(key)
		} else {
			self.app.handle_key(key)
		};
		self.apply(request);
		if self.app.should_quit {
			ctx.exit();
			return;
		}
		self.adopt_path_dialog();
		let force = self.app.take_dirty();
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
		let Self { input, clipboard, app, .. } = self;
		let Some(dialog) = input else {
			return;
		};
		let routed = dialog.ui(app.locale, clipboard).pointer(event, ctx);
		let outcome = dialog.settle(routed, None, path_input::list_dir);
		self.conclude_input(outcome);
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
