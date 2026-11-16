//! The program window and the windows built on widget controls (ADR 0018): the path dialog and the
//! confirmation. The program window holds the open one, hands it keys and the mouse, and acts on
//! what it decides.

use std::path::{Path, PathBuf};

use gterm::app::{Ctx, PointerEvent};
use rust_i18n::t;
use winit::event::ElementState;

use super::Window;
use crate::ui::confirm_dialog::{ConfirmDialog, ConfirmOutcome};
use crate::ui::input_dialog::{InputDialog, InputOutcome};
use crate::ui::path_input::{self, InputKind};
use crate::ui::view::{Size, take_summary};

impl Window {
	/// Shows the input dialog for the path `App` wants asked.
	pub(super) fn adopt_path_request(&mut self) {
		if let Some(request) = self.app.path_request.take() {
			self.input = Some(InputDialog::new(
				request.kind,
				&request.initial,
				&request.history,
				path_input::list_dir,
			));
		}
	}

	/// A key press or release for the open input dialog.
	pub(super) fn key_to_input(&mut self, event: &winit::event::KeyEvent, ctx: &mut Ctx<'_>) {
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
	pub(super) fn pointer_to_input(&mut self, event: &PointerEvent<'_>, ctx: &mut Ctx<'_>) {
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
	pub(super) fn conclude_input(&mut self, outcome: InputOutcome) {
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
			InputKind::MidiFile => self.session.load_midi_file(&path).map_err(|error| error.to_string()),
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

	/// Keeps the confirmation window in step with the question `App` has open.
	pub(super) fn sync_confirm(&mut self) {
		match (self.app.confirm, &self.confirm) {
			(Some(question), Some(dialog)) if dialog.question() == question => {}
			(Some(question), _) => self.confirm = Some(ConfirmDialog::new(question)),
			(None, _) => self.confirm = None,
		}
	}

	/// The running take as the question about deleting it names it; empty while no take runs.
	pub(super) fn take_summary(&self) -> String {
		match (self.session.recording_elapsed(), self.session.recording_file()) {
			(Some(elapsed), Some((_, bytes))) => take_summary(elapsed, Size::of(bytes), self.app.locale),
			_ => String::new(),
		}
	}

	/// A key press or release for the open confirmation window.
	pub(super) fn key_to_confirm(&mut self, event: &winit::event::KeyEvent, ctx: &mut Ctx<'_>) {
		let take = self.take_summary();
		let Some(dialog) = &mut self.confirm else {
			return;
		};
		// The window sees releases too (the library tracks held keys by them); only presses act.
		let routed = dialog.ui(self.app.locale, &take).key(event, ctx);
		if event.state != ElementState::Pressed {
			return;
		}
		let outcome = dialog.settle(routed);
		self.conclude_confirm(outcome, ctx);
	}

	/// A mouse event for the open confirmation window.
	pub(super) fn pointer_to_confirm(&mut self, event: &PointerEvent<'_>, ctx: &mut Ctx<'_>) {
		let take = self.take_summary();
		let Some(dialog) = &mut self.confirm else {
			return;
		};
		let routed = dialog.ui(self.app.locale, &take).pointer(event, ctx);
		let outcome = dialog.settle(routed);
		self.conclude_confirm(outcome, ctx);
	}

	/// Passes the answer of the confirmation window on to `App` and does what it asks for.
	fn conclude_confirm(&mut self, outcome: ConfirmOutcome, ctx: &mut Ctx<'_>) {
		let request = match outcome {
			ConfirmOutcome::Pending => None,
			ConfirmOutcome::Confirmed => self.app.answer(true),
			ConfirmOutcome::Refused => self.app.answer(false),
		};
		self.apply(request);
		if self.app.should_quit {
			ctx.exit();
			return;
		}
		self.sync_confirm();
		self.refresh(ctx, true);
	}
}

/// What a path entered in the folder dialog means: an empty one is the default folder (`Some(None)`),
/// an existing file is not a folder (`None`), anything else is the folder — the recorder creates it
/// when a take starts.
pub(super) fn record_dir_from(path: &Path) -> Option<Option<PathBuf>> {
	if path.as_os_str().is_empty() {
		return Some(None);
	}
	(!path.is_file()).then(|| Some(path.to_owned()))
}
