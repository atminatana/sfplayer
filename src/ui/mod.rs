//! Terminal UI (ratatui + crossterm): state, drawing, key handling. Hotkey policy: ADR 0004; localization: ADR 0008.

pub mod app;
pub mod dialog;
pub mod glyphs;
pub mod i18n;
pub mod keys;
pub mod meter;
pub mod modals;
pub mod monitor;
pub mod path_input;
pub mod screen;
pub mod selector;
pub mod text_keyboard;

pub use app::{App, run};
