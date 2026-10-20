//! Parsing raw MIDI bytes into typed events (spec F-07).
//!
//! `midir` delivers one complete message per callback, so running status is not handled here.
//! Only channel messages the synthesizer acts on get their own variant; everything else is
//! kept as `Other` so the monitor can still log it.

/// Controller numbers the spec requires to be handled (F-07).
pub mod controller {
	pub const MODULATION: u8 = 1;
	pub const VOLUME: u8 = 7;
	pub const PAN: u8 = 10;
	pub const EXPRESSION: u8 = 11;
	pub const SUSTAIN: u8 = 64;
	pub const ALL_SOUND_OFF: u8 = 120;
	pub const RESET_ALL_CONTROLLERS: u8 = 121;
	pub const ALL_NOTES_OFF: u8 = 123;
}

/// Centre of the 14-bit pitch bend range.
pub const PITCH_BEND_CENTRE: u16 = 8192;

/// A channel message. Channels are 0-based (0..=15); data values are 7-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiEvent {
	NoteOn {
		channel: u8,
		key: u8,
		velocity: u8,
	},
	NoteOff {
		channel: u8,
		key: u8,
	},
	ControlChange {
		channel: u8,
		controller: u8,
		value: u8,
	},
	ProgramChange {
		channel: u8,
		program: u8,
	},
	/// 14-bit value 0..=16383, centre 8192.
	PitchBend {
		channel: u8,
		value: u16,
	},
	ChannelPressure {
		channel: u8,
		pressure: u8,
	},
	/// Any other message (polyphonic aftertouch, system messages): only logged.
	Other {
		status: u8,
	},
}

impl MidiEvent {
	/// Channel for channel messages; `None` for system messages.
	pub fn channel(&self) -> Option<u8> {
		match *self {
			Self::NoteOn { channel, .. }
			| Self::NoteOff { channel, .. }
			| Self::ControlChange { channel, .. }
			| Self::ProgramChange { channel, .. }
			| Self::PitchBend { channel, .. }
			| Self::ChannelPressure { channel, .. } => Some(channel),
			Self::Other { status } if status < 0xF0 => Some(status & 0x0F),
			Self::Other { .. } => None,
		}
	}
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MidiParseError {
	#[error("empty MIDI message")]
	Empty,
	#[error("first byte {0:#04x} is not a status byte")]
	MissingStatus(u8),
	#[error("message {status:#04x} needs {expected} data bytes, got {actual}")]
	TooShort { status: u8, expected: usize, actual: usize },
	#[error("data byte {0:#04x} has the high bit set")]
	InvalidDataByte(u8),
}

/// Parses one complete MIDI message. Note On with velocity 0 becomes `NoteOff` (MIDI 1.0 convention).
pub fn parse_message(bytes: &[u8]) -> Result<MidiEvent, MidiParseError> {
	let (&status, data) = bytes.split_first().ok_or(MidiParseError::Empty)?;
	if status < 0x80 {
		return Err(MidiParseError::MissingStatus(status));
	}
	let channel = status & 0x0F;
	let kind = status & 0xF0;

	let event = match kind {
		0x80 => {
			let [key, _velocity] = data_bytes::<2>(status, data)?;
			MidiEvent::NoteOff { channel, key }
		}
		0x90 => {
			let [key, velocity] = data_bytes::<2>(status, data)?;
			if velocity == 0 {
				MidiEvent::NoteOff { channel, key }
			} else {
				MidiEvent::NoteOn { channel, key, velocity }
			}
		}
		0xB0 => {
			let [controller, value] = data_bytes::<2>(status, data)?;
			MidiEvent::ControlChange {
				channel,
				controller,
				value,
			}
		}
		0xC0 => {
			let [program] = data_bytes::<1>(status, data)?;
			MidiEvent::ProgramChange { channel, program }
		}
		0xD0 => {
			let [pressure] = data_bytes::<1>(status, data)?;
			MidiEvent::ChannelPressure { channel, pressure }
		}
		0xE0 => {
			let [lsb, msb] = data_bytes::<2>(status, data)?;
			MidiEvent::PitchBend {
				channel,
				value: u16::from(msb) << 7 | u16::from(lsb),
			}
		}
		_ => MidiEvent::Other { status },
	};
	Ok(event)
}

/// Takes exactly `N` data bytes and validates that each is 7-bit.
fn data_bytes<const N: usize>(status: u8, data: &[u8]) -> Result<[u8; N], MidiParseError> {
	if data.len() < N {
		return Err(MidiParseError::TooShort {
			status,
			expected: N,
			actual: data.len(),
		});
	}
	let mut bytes = [0u8; N];
	for (slot, &byte) in bytes.iter_mut().zip(data) {
		if byte & 0x80 != 0 {
			return Err(MidiParseError::InvalidDataByte(byte));
		}
		*slot = byte;
	}
	Ok(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn note_on() {
		assert_eq!(
			parse_message(&[0x90, 60, 100]),
			Ok(MidiEvent::NoteOn {
				channel: 0,
				key: 60,
				velocity: 100
			})
		);
		assert_eq!(
			parse_message(&[0x9F, 127, 1]),
			Ok(MidiEvent::NoteOn {
				channel: 15,
				key: 127,
				velocity: 1
			})
		);
	}

	#[test]
	fn note_on_with_zero_velocity_is_note_off() {
		assert_eq!(parse_message(&[0x91, 60, 0]), Ok(MidiEvent::NoteOff { channel: 1, key: 60 }));
	}

	#[test]
	fn note_off_ignores_release_velocity() {
		assert_eq!(parse_message(&[0x82, 64, 40]), Ok(MidiEvent::NoteOff { channel: 2, key: 64 }));
	}

	#[test]
	fn control_change() {
		assert_eq!(
			parse_message(&[0xB3, controller::SUSTAIN, 127]),
			Ok(MidiEvent::ControlChange {
				channel: 3,
				controller: 64,
				value: 127
			})
		);
	}

	#[test]
	fn program_change() {
		assert_eq!(parse_message(&[0xC9, 5]), Ok(MidiEvent::ProgramChange { channel: 9, program: 5 }));
	}

	#[test]
	fn channel_pressure() {
		assert_eq!(
			parse_message(&[0xD0, 77]),
			Ok(MidiEvent::ChannelPressure { channel: 0, pressure: 77 })
		);
	}

	#[test]
	fn pitch_bend_combines_lsb_and_msb() {
		assert_eq!(
			parse_message(&[0xE0, 0x00, 0x40]),
			Ok(MidiEvent::PitchBend {
				channel: 0,
				value: PITCH_BEND_CENTRE
			})
		);
		assert_eq!(
			parse_message(&[0xE0, 0x7F, 0x7F]),
			Ok(MidiEvent::PitchBend { channel: 0, value: 16383 })
		);
		assert_eq!(
			parse_message(&[0xE0, 0x00, 0x00]),
			Ok(MidiEvent::PitchBend { channel: 0, value: 0 })
		);
	}

	#[test]
	fn polyphonic_aftertouch_and_system_messages_are_other() {
		assert_eq!(parse_message(&[0xA0, 60, 10]), Ok(MidiEvent::Other { status: 0xA0 }));
		assert_eq!(parse_message(&[0xF8]), Ok(MidiEvent::Other { status: 0xF8 }));
		assert_eq!(parse_message(&[0xF0, 0x7E, 0xF7]), Ok(MidiEvent::Other { status: 0xF0 }));
	}

	#[test]
	fn channel_accessor() {
		assert_eq!(parse_message(&[0x95, 1, 1]).unwrap().channel(), Some(5));
		assert_eq!(parse_message(&[0xA7, 1, 1]).unwrap().channel(), Some(7));
		assert_eq!(parse_message(&[0xF8]).unwrap().channel(), None);
	}

	#[test]
	fn empty_message_is_error() {
		assert_eq!(parse_message(&[]), Err(MidiParseError::Empty));
	}

	#[test]
	fn data_byte_without_status_is_error() {
		assert_eq!(parse_message(&[0x3C, 0x40]), Err(MidiParseError::MissingStatus(0x3C)));
	}

	#[test]
	fn truncated_message_is_error() {
		assert_eq!(
			parse_message(&[0x90, 60]),
			Err(MidiParseError::TooShort {
				status: 0x90,
				expected: 2,
				actual: 1
			})
		);
		assert_eq!(
			parse_message(&[0xC0]),
			Err(MidiParseError::TooShort {
				status: 0xC0,
				expected: 1,
				actual: 0
			})
		);
	}

	#[test]
	fn data_byte_with_high_bit_is_error() {
		assert_eq!(parse_message(&[0x90, 60, 0x80]), Err(MidiParseError::InvalidDataByte(0x80)));
	}

	#[test]
	fn extra_trailing_bytes_are_ignored() {
		assert_eq!(
			parse_message(&[0xC0, 3, 9, 9]),
			Ok(MidiEvent::ProgramChange { channel: 0, program: 3 })
		);
	}
}
