//! The gterm window (spec v1.2 §8): the UI thread is the process main thread running gterm's event loop.
//!
//! All behaviour stays in `App` (key handling) and `fulfil` (requests applied to the `Session`); this
//! module only translates window events into the UI's own key type, drives the periodic work from the
//! tick, and redraws when what the screen shows has changed.

use std::time::{Duration, Instant};

use anyhow::Context;
use gterm::app::{App as GridWindow, AppHandler, AppOptions, Ctx, Grid};
use gterm::buffer::ScreenBuffer;
use gterm::config::{BACKEND_ENV, Config as GridConfig};
use gterm::font::{FONT_SCALE, FontFamily, glyph_px_for};
use gterm::rasterizer::Rasterizer;
use gterm_widgets::{Clipboard, SystemClipboard};
use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::app::{App, FRAME_INTERVAL, Request, fulfil};
use super::chord::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::dialog_screen;
use super::main_screen::{self, BACKGROUND, MIN_COLS, MIN_ROWS, TEXT};
use super::view::View;
use crate::session::{Notice, Session};

const WINDOW_TITLE: &str = "sf2_player";
/// Logical font size when the config does not set one (spec v1.2 §7).
pub const DEFAULT_FONT_SIZE_PX: u16 = 14;
/// Half of the blink period of the recording dot: a new frame is due whenever this phase flips.
const BLINK_HALF_PERIOD_MS: u128 = 500;
/// Function keys by number: `FUNCTION_KEYS[0]` is F1.
const FUNCTION_KEYS: [NamedKey; 24] = [
	NamedKey::F1,
	NamedKey::F2,
	NamedKey::F3,
	NamedKey::F4,
	NamedKey::F5,
	NamedKey::F6,
	NamedKey::F7,
	NamedKey::F8,
	NamedKey::F9,
	NamedKey::F10,
	NamedKey::F11,
	NamedKey::F12,
	NamedKey::F13,
	NamedKey::F14,
	NamedKey::F15,
	NamedKey::F16,
	NamedKey::F17,
	NamedKey::F18,
	NamedKey::F19,
	NamedKey::F20,
	NamedKey::F21,
	NamedKey::F22,
	NamedKey::F23,
	NamedKey::F24,
];

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
		let Some(key) = key_from_window(&event.logical_key, event.state, event.repeat, ctx.modifiers) else {
			return;
		};
		// Raw key stream for diagnosing input (auto-repeat, release events); spec N-07.
		tracing::debug!(code = ?key.code, kind = ?key.kind, modifiers = ?key.modifiers, "key event");
		let request = if is_paste_chord(&key) && self.app.modal.is_some() {
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
		let force = self.app.take_dirty();
		self.refresh(ctx, force);
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
		let now = Instant::now();
		self.app.recording = self.session.recording_elapsed();
		let request = self.app.tick_keyboard(now);
		self.apply(request);
		self.app.update_meters(self.session.shared.clips(), now);
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

/// Ctrl+V or Shift+Insert, pressed: paste the clipboard into the field being edited.
fn is_paste_chord(key: &KeyEvent) -> bool {
	if key.kind == KeyEventKind::Release {
		return false;
	}
	match key.code {
		KeyCode::Char('v' | 'V') => key.modifiers == KeyModifiers::CONTROL,
		KeyCode::Insert => key.modifiers == KeyModifiers::SHIFT,
		_ => false,
	}
}

/// Translates a window key event into the UI's own type; `None` for keys the UI does not distinguish.
///
/// The logical key is used, so a chord means the same as in a terminal: `Shift` + `=` is `+`, a letter
/// comes with the case the layout gives it.
fn key_from_window(logical_key: &Key, state: ElementState, is_repeat: bool, held: ModifiersState) -> Option<KeyEvent> {
	let mut modifiers = KeyModifiers::NONE;
	for (is_held, modifier) in [
		(held.shift_key(), KeyModifiers::SHIFT),
		(held.control_key(), KeyModifiers::CONTROL),
		(held.alt_key(), KeyModifiers::ALT),
		(held.super_key(), KeyModifiers::SUPER),
	] {
		if is_held {
			modifiers |= modifier;
		}
	}
	let code = match logical_key {
		Key::Named(named) => match named {
			NamedKey::ArrowUp => KeyCode::Up,
			NamedKey::ArrowDown => KeyCode::Down,
			NamedKey::ArrowLeft => KeyCode::Left,
			NamedKey::ArrowRight => KeyCode::Right,
			NamedKey::Home => KeyCode::Home,
			NamedKey::End => KeyCode::End,
			NamedKey::PageUp => KeyCode::PageUp,
			NamedKey::PageDown => KeyCode::PageDown,
			NamedKey::Enter => KeyCode::Enter,
			NamedKey::Escape => KeyCode::Esc,
			NamedKey::Tab if held.shift_key() => KeyCode::BackTab,
			NamedKey::Tab => KeyCode::Tab,
			NamedKey::Backspace => KeyCode::Backspace,
			NamedKey::Delete => KeyCode::Delete,
			NamedKey::Insert => KeyCode::Insert,
			NamedKey::Space => KeyCode::Char(' '),
			other => {
				let index = FUNCTION_KEYS.iter().position(|function_key| function_key == other)?;
				KeyCode::F(u8::try_from(index + 1).ok()?)
			}
		},
		Key::Character(text) => {
			let mut characters = text.chars();
			let character = characters.next()?;
			if characters.next().is_some() {
				return None;
			}
			KeyCode::Char(character)
		}
		_ => return None,
	};
	let kind = match (state, is_repeat) {
		(ElementState::Released, _) => KeyEventKind::Release,
		(ElementState::Pressed, true) => KeyEventKind::Repeat,
		(ElementState::Pressed, false) => KeyEventKind::Press,
	};
	Some(KeyEvent { code, modifiers, kind })
}

#[cfg(test)]
mod tests;
