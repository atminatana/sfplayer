//! MIDI layer: message parsing (parse.rs), midir ports (ports.rs), hot-plug polling (hotplug.rs) and
//! Standard MIDI Files: reading (smf.rs), writing (smf/write.rs) and playing (player.rs).

mod hotplug;
mod parse;
pub mod player;
mod ports;
pub mod smf;

pub use hotplug::{DEFAULT_POLL_INTERVAL, HotplugWatcher, MidiStatus};
pub use parse::{MidiEvent, MidiParseError, PITCH_BEND_CENTRE, controller, parse_message};
pub use ports::{MidiError, MidiInputHandle, MidiPortInfo, connect_input, list_input_ports};
