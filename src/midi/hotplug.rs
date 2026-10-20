//! Hot-plug detection by polling (spec §9: midir does not notify about device changes).
//!
//! A background thread re-enumerates input ports every `interval` and reports the new list whenever
//! it differs from the previous one. The connection status is derived from the list by the caller.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{MidiError, MidiPortInfo, list_input_ports};

/// Spec §9: re-scan every 2 seconds.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// State of the MIDI input as shown in the status line (spec F-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiStatus {
	Connected(String),
	/// The port we were connected to is no longer enumerated.
	Disconnected(String),
	NoPort,
}

impl MidiStatus {
	/// Derives the status from the connected port name and the current port list.
	pub fn from_ports(connected: Option<&str>, ports: &[MidiPortInfo]) -> Self {
		match connected {
			None => Self::NoPort,
			Some(name) if ports.iter().any(|port| port.name == name) => Self::Connected(name.to_owned()),
			Some(name) => Self::Disconnected(name.to_owned()),
		}
	}
}

/// Owns the polling thread; dropping it stops the thread promptly (no wait for the full interval).
pub struct HotplugWatcher {
	stop: Option<Sender<()>>,
	thread: Option<JoinHandle<()>>,
}

impl HotplugWatcher {
	/// Starts polling with the real midir enumeration.
	pub fn spawn(interval: Duration, initial: Vec<MidiPortInfo>, on_change: impl FnMut(Vec<MidiPortInfo>) + Send + 'static) -> Self {
		Self::spawn_with(interval, initial, list_input_ports, on_change)
	}

	/// Starts polling with a custom enumeration function (used by tests).
	pub fn spawn_with(
		interval: Duration,
		initial: Vec<MidiPortInfo>,
		mut list_ports: impl FnMut() -> Result<Vec<MidiPortInfo>, MidiError> + Send + 'static,
		mut on_change: impl FnMut(Vec<MidiPortInfo>) + Send + 'static,
	) -> Self {
		let (stop, stop_signal) = mpsc::channel::<()>();
		let thread = std::thread::Builder::new()
			.name("midi-hotplug".to_owned())
			.spawn(move || {
				let mut known = initial;
				loop {
					match stop_signal.recv_timeout(interval) {
						Err(RecvTimeoutError::Timeout) => {}
						Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
					}
					match list_ports() {
						Ok(ports) if ports != known => {
							known = ports.clone();
							on_change(ports);
						}
						Ok(_) => {}
						Err(error) => tracing::warn!(%error, "MIDI port scan failed"),
					}
				}
			})
			.expect("spawning the MIDI hot-plug thread");
		Self {
			stop: Some(stop),
			thread: Some(thread),
		}
	}
}

impl Drop for HotplugWatcher {
	fn drop(&mut self) {
		drop(self.stop.take());
		if let Some(thread) = self.thread.take() {
			let _ = thread.join();
		}
	}
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::sync::{Arc, Mutex};

	use super::*;

	const POLL: Duration = Duration::from_millis(5);
	const WAIT: Duration = Duration::from_secs(5);

	fn port(index: usize, name: &str) -> MidiPortInfo {
		MidiPortInfo {
			index,
			name: name.to_owned(),
		}
	}

	#[test]
	fn status_from_ports() {
		let ports = vec![port(0, "KeyStep")];
		assert_eq!(MidiStatus::from_ports(None, &ports), MidiStatus::NoPort);
		assert_eq!(
			MidiStatus::from_ports(Some("KeyStep"), &ports),
			MidiStatus::Connected("KeyStep".into())
		);
		assert_eq!(
			MidiStatus::from_ports(Some("KeyStep"), &[]),
			MidiStatus::Disconnected("KeyStep".into())
		);
	}

	#[test]
	fn reports_only_when_the_list_changes() {
		let scans = Arc::new(AtomicUsize::new(0));
		let scans_in_thread = Arc::clone(&scans);
		let (changes, changes_rx) = mpsc::channel();
		let lists = Arc::new(Mutex::new(vec![
			vec![port(0, "A")],
			vec![port(0, "A")],
			vec![port(0, "A"), port(1, "B")],
			vec![],
		]));
		let lists_in_thread = Arc::clone(&lists);

		let watcher = HotplugWatcher::spawn_with(
			POLL,
			vec![port(0, "A")],
			move || {
				let scan = scans_in_thread.fetch_add(1, Ordering::SeqCst);
				let lists = lists_in_thread.lock().unwrap();
				Ok(lists[scan.min(lists.len() - 1)].clone())
			},
			move |ports| changes.send(ports).unwrap(),
		);

		let first = changes_rx.recv_timeout(WAIT).expect("first change");
		assert_eq!(first, vec![port(0, "A"), port(1, "B")]);
		let second = changes_rx.recv_timeout(WAIT).expect("second change");
		assert!(second.is_empty());
		assert!(changes_rx.recv_timeout(Duration::from_millis(50)).is_err(), "no further changes");
		drop(watcher);
		assert!(scans.load(Ordering::SeqCst) >= 3);
	}

	#[test]
	fn scan_errors_are_tolerated_and_drop_stops_the_thread() {
		let (changes, changes_rx) = mpsc::channel::<Vec<MidiPortInfo>>();
		let watcher = HotplugWatcher::spawn_with(
			POLL,
			vec![],
			|| Err(MidiError::PortNotFound { index: 0 }),
			move |ports| changes.send(ports).unwrap(),
		);
		assert!(changes_rx.recv_timeout(Duration::from_millis(50)).is_err());
		let started = std::time::Instant::now();
		drop(watcher);
		assert!(started.elapsed() < Duration::from_secs(1), "drop must not wait for the interval");
	}
}
