//! MIDI input ports via midir (spec F-01, F-02): enumeration and a connection that feeds parsed events to a sink.

use midir::{ConnectErrorKind, Ignore, MidiInput, MidiInputConnection};

use super::{MidiEvent, parse_message};

const CLIENT_NAME: &str = "sf2_player";

#[derive(Debug, thiserror::Error)]
pub enum MidiError {
	#[error("cannot initialise the MIDI subsystem")]
	Init(#[source] midir::InitError),
	#[error("cannot read MIDI port name")]
	PortName(#[source] midir::PortInfoError),
	#[error("MIDI input port {index} does not exist")]
	PortNotFound { index: usize },
	#[error("cannot connect to MIDI port {port}: {kind}")]
	Connect { port: String, kind: ConnectErrorKind },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortInfo {
	/// Position in the enumeration order; what `--midi-in INDEX` refers to.
	pub index: usize,
	pub name: String,
}

pub fn list_input_ports() -> Result<Vec<MidiPortInfo>, MidiError> {
	let input = MidiInput::new(CLIENT_NAME).map_err(MidiError::Init)?;
	input
		.ports()
		.iter()
		.enumerate()
		.map(|(index, port)| {
			Ok(MidiPortInfo {
				index,
				name: input.port_name(port).map_err(MidiError::PortName)?,
			})
		})
		.collect()
}

/// An open input connection; dropping it closes the port.
pub struct MidiInputHandle {
	_connection: MidiInputConnection<()>,
	pub port_name: String,
}

/// Connects to the port at `index` and delivers every parsed message to `sink` on midir's thread.
/// Unparseable messages are logged at debug level and dropped.
pub fn connect_input(index: usize, mut sink: impl FnMut(MidiEvent) + Send + 'static) -> Result<MidiInputHandle, MidiError> {
	let mut input = MidiInput::new(CLIENT_NAME).map_err(MidiError::Init)?;
	input.ignore(Ignore::None);
	let ports = input.ports();
	let port = ports.get(index).ok_or(MidiError::PortNotFound { index })?;
	let port_name = input.port_name(port).map_err(MidiError::PortName)?;

	let callback = move |_timestamp: u64, bytes: &[u8], _: &mut ()| match parse_message(bytes) {
		Ok(event) => sink(event),
		Err(error) => tracing::debug!(%error, ?bytes, "ignoring MIDI message"),
	};
	let connection = input.connect(port, CLIENT_NAME, callback, ()).map_err(|error| MidiError::Connect {
		port: port_name.clone(),
		kind: error.kind(),
	})?;
	tracing::info!(port = %port_name, "MIDI input connected");
	Ok(MidiInputHandle {
		_connection: connection,
		port_name,
	})
}
