//! Terminal UI (ratatui + crossterm): state, drawing, key handling. Hotkey policy: ADR 0004; localization: ADR 0008.

pub mod app;
pub mod chord;
pub mod dialog;
pub mod dialog_screen;
pub mod effects_dialog;
pub mod glyphs;
pub mod i18n;
pub mod keys;
pub mod main_screen;
pub mod meter;
pub mod modals;
pub mod paint;
pub mod path_input;
pub mod record_dialog;
pub mod screen;
pub mod selector;
pub mod text_keyboard;
pub mod view;
pub mod window;

pub use app::{App, run};
