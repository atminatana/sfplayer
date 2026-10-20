//! MIDI input layer: message parsing (parse.rs) and midir ports (ports.rs); hot-plug polling arrives in step 2.3.

mod parse;
mod ports;

pub use parse::{MidiEvent, MidiParseError, PITCH_BEND_CENTRE, controller, parse_message};
pub use ports::{MidiError, MidiInputHandle, MidiPortInfo, connect_input, list_input_ports};
