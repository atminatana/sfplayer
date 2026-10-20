//! MIDI input layer: message parsing (parse.rs); ports and hot-plug arrive in later steps.

mod parse;

pub use parse::{MidiEvent, MidiParseError, PITCH_BEND_CENTRE, controller, parse_message};
