//! MIDI input layer: message parsing (parse.rs), midir ports (ports.rs) and hot-plug polling (hotplug.rs).

mod hotplug;
mod parse;
mod ports;

pub use hotplug::{DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiStatus};
pub use parse::{MidiEvent, MidiParseError, PITCH_BEND_CENTRE, controller, parse_message};
pub use ports::{MidiError, MidiInputHandle, MidiPortInfo, connect_input, list_input_ports};
