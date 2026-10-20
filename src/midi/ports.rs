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

/// An open input connection owning the sink state `T`; `close` hands `T` back so it can be reused.
pub struct MidiInputHandle<T: 'static> {
	connection: MidiInputConnection<T>,
	pub port_name: String,
}

impl<T> MidiInputHandle<T> {
	pub fn close(self) -> T {
		self.connection.close().1
	}
}

/// Connects to the port at `index`; every parsed message is passed to `sink` together with `state` on midir's thread.
/// Unparseable messages are logged at debug level and dropped.
pub fn connect_input<T: Send + 'static>(
	index: usize,
	state: T,
	mut sink: impl FnMut(MidiEvent, &mut T) + Send + 'static,
) -> Result<MidiInputHandle<T>, MidiError> {
	let mut input = MidiInput::new(CLIENT_NAME).map_err(MidiError::Init)?;
	input.ignore(Ignore::None);
	let ports = input.ports();
	let port = ports.get(index).ok_or(MidiError::PortNotFound { index })?;
	let port_name = input.port_name(port).map_err(MidiError::PortName)?;

	let callback = move |_timestamp: u64, bytes: &[u8], state: &mut T| match parse_message(bytes) {
		Ok(event) => sink(event, state),
		Err(error) => tracing::debug!(%error, ?bytes, "ignoring MIDI message"),
	};
	let connection = input
		.connect(port, CLIENT_NAME, callback, state)
		.map_err(|error| MidiError::Connect {
			port: port_name.clone(),
			kind: error.kind(),
		})?;
	tracing::info!(port = %port_name, "MIDI input connected");
	Ok(MidiInputHandle { connection, port_name })
}
