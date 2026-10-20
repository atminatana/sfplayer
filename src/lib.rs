//! sf2_player — console SoundFont synthesizer. Library crate: modules used by the binary and by tests.

rust_i18n::i18n!("locales", fallback = "en");

pub mod audio;
pub mod cli;
pub mod config;
pub mod logging;
pub mod midi;
pub mod session;
pub mod synth;
pub mod ui;
