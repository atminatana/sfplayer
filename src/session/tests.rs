//! The session with no device opened: what its recording and MIDI port methods do while there is no
//! audio stream and no port. The paths that need a real device cannot be reached from a test.

use super::*;

/// A session that has opened nothing; nothing here saves its config, so the file is never written.
fn idle_session(config: Config) -> Session {
	Session::detached(config, PathBuf::from("never-written.toml"))
}

#[test]
fn takes_go_to_the_folder_from_the_config() {
	let mut config = Config::default();
	config.record.dir = Some(PathBuf::from("chosen-folder"));
	assert_eq!(idle_session(config).record_dir(), PathBuf::from("chosen-folder"));
}

/// Which system folder is found depends on the machine; the last component is the program's own.
#[test]
fn without_a_configured_folder_takes_go_to_a_folder_of_the_program() {
	let dir = idle_session(Config::default()).record_dir();
	let last = dir.file_name().map(|name| name.to_string_lossy().into_owned());
	assert!(
		matches!(last.as_deref(), Some("sf2_player" | "recordings")),
		"unexpected default folder: {}",
		dir.display()
	);
}

#[test]
fn a_take_does_not_start_without_an_audio_stream() {
	let mut session = idle_session(Config::default());
	session.start_recording();
	assert_eq!(session.recording_elapsed(), None);
	assert_eq!(session.recording_file(), None);
	assert_eq!(session.notice, None);
}

#[test]
fn stopping_and_cancelling_do_nothing_when_no_take_runs() {
	let mut session = idle_session(Config::default());
	session.stop_recording();
	session.cancel_recording();
	assert_eq!(session.notice, None);
	assert!(session.last_take.is_none());
}

#[test]
fn an_idle_session_has_no_midi_port() {
	let session = idle_session(Config::default());
	assert_eq!(session.connected_midi_port(), None);
	assert_eq!(session.midi_status, MidiStatus::NoPort);
}

/// The ring to the audio thread appears with the stream: without it there is nothing to connect a port to.
#[test]
fn connecting_a_port_without_an_audio_stream_changes_nothing() {
	let mut session = idle_session(Config::default());
	session.connect_midi(0);
	assert_eq!(session.connected_midi_port(), None);
	assert_eq!(session.midi_status, MidiStatus::NoPort);
	assert_eq!(session.config.midi.input, None);
	assert_eq!(session.notice, None);
}

#[test]
fn disconnecting_leaves_no_port() {
	let mut session = idle_session(Config::default());
	session.midi_status = MidiStatus::Disconnected("gone".to_owned());
	session.disconnect_midi();
	assert_eq!(session.midi_status, MidiStatus::NoPort);
	assert_eq!(session.connected_midi_port(), None);
}
