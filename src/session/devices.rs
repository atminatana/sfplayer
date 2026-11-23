//! The devices of the session: the audio output stream and the MIDI input port — opening, switching,
//! re-enumerating, connecting.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use ringbuf::traits::Producer;

use super::{MidiSink, Notice, Session};
use crate::audio::{self, AudioLinks, OutputDevice};
use crate::cli::DeviceSelector;
use crate::midi::player::Player;
use crate::midi::{self, MidiStatus};

impl Session {
	pub(super) fn open_audio(&mut self, device: &OutputDevice) {
		match audio::open_output_stream(device, self.params, &self.shared) {
			Ok((
				stream,
				AudioLinks {
					events,
					file_events,
					commands,
					retired,
					recording,
				},
			)) => {
				self.audio = Some(stream);
				self.commands = Some(commands);
				self.retired = Some(retired);
				self.record_rx = Some(recording);
				self.spare_events = Some(events);
				// A file that is playing goes on into the ring of the new stream.
				match &self.player {
					Some(player) => player.set_output(file_events),
					None => {
						let player = Player::spawn(file_events);
						if let Some(song) = &self.song {
							player.load(Arc::clone(song));
						}
						self.player = Some(player);
					}
				}
			}
			Err(error) => self.notice = Some(Notice::NoAudioDevice(error.to_string())),
		}
	}

	/// Switches to another output device (F-03): new stream and rings, engine re-created for the new rate,
	/// MIDI reconnected to the new ring, the preset re-applied.
	pub fn reopen_audio(&mut self, index: usize) {
		let device = match audio::output_device_by_index(index) {
			Ok(device) => device,
			Err(error) => {
				self.notice = Some(Notice::AudioError(error.to_string()));
				return;
			}
		};
		let midi_index = self.midi_port_index;
		self.stop_recording();
		self.disconnect_midi();
		self.audio = None;
		self.commands = None;
		self.retired = None;
		self.record_rx = None;
		self.spare_events = None;
		self.open_audio(&device);
		let Some(stream) = &self.audio else {
			return;
		};
		let label = stream.label.clone();
		self.config.audio.output = Some(label.clone());
		self.notice = Some(Notice::AudioReopened(label));
		if let Err(error) = self.install_engine() {
			self.notice = Some(Notice::Sf2Error(error.to_string()));
		}
		if let Some(index) = midi_index {
			self.connect_midi(index);
		}
	}

	/// Resolves the selector against the known ports: no match → first port with a notice; no ports → `None`.
	pub(super) fn pick_midi_port(&mut self, selector: Option<&DeviceSelector>) -> Option<usize> {
		let first = self.midi_ports.first().map(|port| port.index);
		let selector = selector?;
		match selector.find(self.midi_ports.iter().map(|port| port.name.as_str())) {
			Some(index) => Some(index),
			None => {
				tracing::warn!(?selector, "MIDI input not found, using the first available port");
				self.notice = Some(Notice::MidiFallback);
				first
			}
		}
	}

	/// Re-enumerates MIDI ports (F8) and refreshes the connection status.
	pub fn rescan(&mut self) {
		match midi::list_input_ports() {
			Ok(ports) => {
				self.midi_ports = ports;
				self.refresh_midi_status();
			}
			Err(error) => self.notice = Some(Notice::MidiError(error.to_string())),
		}
	}

	pub(super) fn refresh_midi_status(&mut self) {
		let connected = self.midi.as_ref().map(|handle| handle.port_name.clone());
		self.midi_status = MidiStatus::from_ports(connected.as_deref(), &self.midi_ports);
	}

	/// Connects to the MIDI port, moving the event producer into the midir callback.
	pub fn connect_midi(&mut self, index: usize) {
		self.disconnect_midi();
		let Some(events) = self.spare_events.take() else {
			return;
		};
		let sink = MidiSink {
			events,
			dropped: Arc::clone(&self.midi_dropped),
		};
		match midi::connect_input(index, sink, |event, sink| {
			if sink.events.try_push(event).is_err() {
				sink.dropped.fetch_add(1, Ordering::Relaxed);
			}
		}) {
			Ok(handle) => {
				self.midi_status = MidiStatus::Connected(handle.port_name.clone());
				self.config.midi.input = Some(handle.port_name.clone());
				self.midi = Some(handle);
				self.midi_port_index = Some(index);
			}
			Err(error) => {
				tracing::warn!(%error, index, "MIDI connect failed");
				self.notice = Some(Notice::MidiError(error.to_string()));
			}
		}
	}

	pub fn disconnect_midi(&mut self) {
		if let Some(handle) = self.midi.take() {
			let sink = handle.close();
			self.spare_events = Some(sink.events);
		}
		self.midi_port_index = None;
		self.midi_status = MidiStatus::NoPort;
	}

	pub fn connected_midi_port(&self) -> Option<&str> {
		self.midi.as_ref().map(|handle| handle.port_name.as_str())
	}
}
