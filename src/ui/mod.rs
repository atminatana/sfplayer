//! Terminal UI (ratatui + crossterm): state, drawing, key handling. Hotkey policy: ADR 0004; localization: ADR 0008.

pub mod app;
pub mod i18n;
pub mod keys;
pub mod screen;

pub use app::{App, run};
