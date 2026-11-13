//! The window UI on gterm (ADR 0012): state, drawing, key handling. Hotkey policy: ADR 0004; localization: ADR 0008.

pub mod app;
pub mod buttons;
pub mod chord;
pub mod dialog;
pub mod dialog_screen;
pub mod effects_dialog;
pub mod i18n;
pub mod input_dialog;
pub mod keymap;
pub mod keys;
pub mod main_screen;
pub mod meter;
pub mod paint;
pub mod path_input;
pub mod piano;
pub mod selector;
pub mod text_keyboard;
pub mod view;
pub mod window;

pub use app::App;
