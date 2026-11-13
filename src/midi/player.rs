//! Playing a MIDI file (spec v1.2, F-21): a sequencer that hands the messages of a `Song` over when
//! they are due, and the thread that runs it against the clock.
//!
//! The messages go into a ring of their own to the audio thread, which takes them the way it takes the
//! MIDI input. `Sequencer` is the logic — which message is due, what to send when playback pauses or
//! stops — and knows no thread and no clock: time is given to it, so it is tested with made-up time.
//! `Player` is the thread around it.
//!
//! The thread waits with `thread::sleep` in short slices and looks at its command channel without
//! blocking. A wait on the channel itself would be simpler, but on Windows such waits wake on the
//! system timer tick: measured on the development machine, a 1 ms channel wait takes 15.6 ms and a
//! 1 ms sleep 1.8 ms. Notes arriving up to 15 ms late are audible.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ringbuf::HeapProd;
use ringbuf::traits::Producer;

use super::parse::{MidiEvent, controller};
use super::smf::Song;

const CHANNELS: usize = 16;
/// CC 64 values from here up mean the pedal is down.
const PEDAL_DOWN: u8 = 64;
/// The longest the thread sleeps in one go while a song plays: commands are noticed this soon.
const SLICE: Duration = Duration::from_millis(2);
/// How soon a message that did not fit into the ring is offered again.
const RETRY: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
	/// At the start of the song, or no song at all.
	Stopped,
	Playing,
	/// Held at a position; play goes on from there.
	Paused,
}

/// What `Sequencer::poll` found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Poll {
	/// Nothing is playing and nothing waits to be sent.
	Idle,
	/// The next message is due after this long.
	Wait(Duration),
	/// The song has just played to its end and stopped by itself.
	Finished,
}

/// Where a message goes. `false` means it could not be taken now and is to be offered again.
pub type Sink<'a> = &'a mut dyn FnMut(MidiEvent) -> bool;

pub struct Sequencer {
	song: Option<Arc<Song>>,
	transport: Transport,
	/// Index of the next message of the song to send.
	next: usize,
	/// The clock time at which the song was, or would have been, at its start — while playing.
	origin: Duration,
	/// Seconds into the song where it is paused.
	paused_at: f64,
	/// Notes the song has started and not ended: a bit per key, per channel.
	held: [u128; CHANNELS],
	/// Channels on which the song holds the pedal down.
	pedals: u16,
	/// Whether what is held is to be let go: playback has paused or stopped.
	is_releasing: bool,
}

impl Default for Sequencer {
	fn default() -> Self {
		Self::new()
	}
}

impl Sequencer {
	pub fn new() -> Self {
		Self {
			song: None,
			transport: Transport::Stopped,
			next: 0,
			origin: Duration::ZERO,
			paused_at: 0.0,
			held: [0; CHANNELS],
			pedals: 0,
			is_releasing: false,
		}
	}

	pub fn transport(&self) -> Transport {
		self.transport
	}

	/// Length of the loaded song in seconds; 0 without one.
	pub fn duration(&self) -> f64 {
		self.song.as_ref().map_or(0.0, |song| song.duration)
	}

	/// Seconds into the song at clock time `now`.
	pub fn position(&self, now: Duration) -> f64 {
		match self.transport {
			Transport::Stopped => 0.0,
			Transport::Paused => self.paused_at,
			Transport::Playing => now.saturating_sub(self.origin).as_secs_f64().min(self.duration()),
		}
	}

	/// Takes another song. One that is playing is stopped first; the new one waits at its start.
	pub fn load(&mut self, song: Arc<Song>) {
		self.stop();
		self.song = Some(song);
	}

	/// Starts from the beginning, or goes on from where the song is paused. Without a song, and while
	/// playing, nothing changes.
	pub fn play(&mut self, now: Duration) {
		if self.song.is_none() {
			return;
		}
		match self.transport {
			Transport::Playing => {}
			// A stopped song is at its first message already: `stop` put it there.
			Transport::Stopped => {
				self.origin = now;
				self.transport = Transport::Playing;
			}
			Transport::Paused => {
				self.origin = now.saturating_sub(Duration::from_secs_f64(self.paused_at));
				self.transport = Transport::Playing;
			}
		}
	}

	/// Holds the song where it is; the notes it sounds are let go by the next `poll`.
	pub fn pause(&mut self, now: Duration) {
		if self.transport == Transport::Playing {
			self.paused_at = self.position(now);
			self.transport = Transport::Paused;
			self.is_releasing = true;
		}
	}

	/// Back to the start; the notes the song sounds are let go by the next `poll`. Returns whether
	/// there was anything to stop.
	pub fn stop(&mut self) -> bool {
		let was_running = self.transport != Transport::Stopped;
		self.transport = Transport::Stopped;
		self.next = 0;
		self.paused_at = 0.0;
		self.is_releasing = true;
		was_running
	}

	/// Sends what is due at clock time `now` and tells what to wait for.
	pub fn poll(&mut self, now: Duration, sink: Sink<'_>) -> Poll {
		if self.is_releasing {
			if !self.release(sink) {
				return Poll::Wait(RETRY);
			}
			self.is_releasing = false;
		}
		if self.transport != Transport::Playing {
			return Poll::Idle;
		}
		let Some(song) = self.song.clone() else {
			return Poll::Idle;
		};
		let position = now.saturating_sub(self.origin).as_secs_f64();
		while let Some(timed) = song.events.get(self.next) {
			if timed.seconds > position {
				return Poll::Wait(Duration::from_secs_f64(timed.seconds - position));
			}
			if !sink(timed.event) {
				return Poll::Wait(RETRY);
			}
			self.remember(timed.event);
			self.next += 1;
		}
		// Every message is out; the song lasts until its longest track ends.
		if position < song.duration {
			return Poll::Wait(Duration::from_secs_f64(song.duration - position));
		}
		self.stop();
		// A file may end with notes still on; they do not outlive it.
		if self.release(sink) {
			self.is_releasing = false;
		}
		Poll::Finished
	}

	/// Keeps track of what the song holds, so that it can be let go when playback is interrupted.
	fn remember(&mut self, event: MidiEvent) {
		match event {
			MidiEvent::NoteOn { channel, key, .. } => {
				if let Some(keys) = self.held.get_mut(usize::from(channel)) {
					*keys |= 1u128 << (key & 0x7F);
				}
			}
			MidiEvent::NoteOff { channel, key } => {
				if let Some(keys) = self.held.get_mut(usize::from(channel)) {
					*keys &= !(1u128 << (key & 0x7F));
				}
			}
			MidiEvent::ControlChange {
				channel,
				controller: controller::SUSTAIN,
				value,
			} if usize::from(channel) < CHANNELS => {
				if value >= PEDAL_DOWN {
					self.pedals |= 1 << channel;
				} else {
					self.pedals &= !(1 << channel);
				}
			}
			_ => {}
		}
	}

	/// Lets go of every pedal and every note the song holds. `false` when the sink refused a message:
	/// what was not sent stays held and is offered again.
	fn release(&mut self, sink: Sink<'_>) -> bool {
		for channel in 0..CHANNELS as u8 {
			let pedal = 1u16 << channel;
			if self.pedals & pedal != 0 {
				let pedal_up = MidiEvent::ControlChange {
					channel,
					controller: controller::SUSTAIN,
					value: 0,
				};
				if !sink(pedal_up) {
					return false;
				}
				self.pedals &= !pedal;
			}
			while self.held[usize::from(channel)] != 0 {
				let key = self.held[usize::from(channel)].trailing_zeros() as u8;
				if !sink(MidiEvent::NoteOff { channel, key }) {
					return false;
				}
				self.held[usize::from(channel)] &= !(1u128 << key);
			}
		}
		true
	}
}

/// What the UI reads about playback, published by the player thread through atomics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerStatus {
	pub transport: Transport,
	pub position: Duration,
	pub duration: Duration,
}

struct PlayerShared {
	transport: AtomicU8,
	position_ms: AtomicU32,
	duration_ms: AtomicU32,
	/// Times playback came to a stop: by a stop command or at the end of a song.
	stops: AtomicU32,
}

enum Control {
	Load(Arc<Song>),
	Play,
	Pause,
	Stop,
	/// Another ring to send into: the audio stream was reopened.
	Output(HeapProd<MidiEvent>),
	Shutdown,
}

/// The sequencer on a thread of its own, sending into the ring to the audio thread.
pub struct Player {
	control: Sender<Control>,
	shared: Arc<PlayerShared>,
	thread: Option<JoinHandle<()>>,
}

impl Player {
	/// Starts the thread; it sleeps until told to play.
	pub fn spawn(output: HeapProd<MidiEvent>) -> Self {
		let started = Instant::now();
		Self::spawn_with(output, move || started.elapsed())
	}

	/// The same with a clock of the caller's choosing (tests move time by hand).
	pub fn spawn_with(output: HeapProd<MidiEvent>, clock: impl Fn() -> Duration + Send + 'static) -> Self {
		let (control, commands) = mpsc::channel();
		let shared = Arc::new(PlayerShared {
			transport: AtomicU8::new(Transport::Stopped as u8),
			position_ms: AtomicU32::new(0),
			duration_ms: AtomicU32::new(0),
			stops: AtomicU32::new(0),
		});
		let published = Arc::clone(&shared);
		let thread = std::thread::Builder::new()
			.name("midi-file".to_owned())
			.spawn(move || run(output, &commands, &published, &clock))
			.ok();
		if thread.is_none() {
			tracing::error!("cannot start the MIDI file thread; files will not play");
		}
		Self { control, shared, thread }
	}

	pub fn load(&self, song: Arc<Song>) {
		self.send(Control::Load(song));
	}

	pub fn play(&self) {
		self.send(Control::Play);
	}

	pub fn pause(&self) {
		self.send(Control::Pause);
	}

	pub fn stop(&self) {
		self.send(Control::Stop);
	}

	/// Gives the player the ring of a reopened audio stream; playback goes on into it.
	pub fn set_output(&self, output: HeapProd<MidiEvent>) {
		self.send(Control::Output(output));
	}

	pub fn status(&self) -> PlayerStatus {
		let transport = match self.shared.transport.load(Ordering::Relaxed) {
			value if value == Transport::Playing as u8 => Transport::Playing,
			value if value == Transport::Paused as u8 => Transport::Paused,
			_ => Transport::Stopped,
		};
		PlayerStatus {
			transport,
			position: Duration::from_millis(u64::from(self.shared.position_ms.load(Ordering::Relaxed))),
			duration: Duration::from_millis(u64::from(self.shared.duration_ms.load(Ordering::Relaxed))),
		}
	}

	/// How many times playback has come to a stop. When the number changes, the song is over or was
	/// stopped, and whatever it changed in the synthesizer can be put back.
	pub fn stops(&self) -> u32 {
		self.shared.stops.load(Ordering::Relaxed)
	}

	fn send(&self, control: Control) {
		// The thread is gone only if it could not be started or has panicked; there is nobody to tell.
		let _ = self.control.send(control);
	}
}

impl Drop for Player {
	fn drop(&mut self) {
		self.send(Control::Shutdown);
		if let Some(thread) = self.thread.take() {
			let _ = thread.join();
		}
	}
}

/// The body of the player thread.
fn run(mut output: HeapProd<MidiEvent>, commands: &Receiver<Control>, shared: &PlayerShared, clock: &dyn Fn() -> Duration) {
	let mut sequencer = Sequencer::new();
	loop {
		// Everything that has been asked for so far, without waiting.
		loop {
			match commands.try_recv() {
				Ok(control) => {
					if !apply(control, &mut sequencer, &mut output, shared, clock()) {
						return;
					}
				}
				Err(TryRecvError::Empty) => break,
				Err(TryRecvError::Disconnected) => return,
			}
		}
		let now = clock();
		let poll = sequencer.poll(now, &mut |event| output.try_push(event).is_ok());
		publish(&sequencer, shared, now);
		match poll {
			// Nothing to do until told otherwise: this wait costs no processor time.
			Poll::Idle => match commands.recv() {
				Ok(control) => {
					if !apply(control, &mut sequencer, &mut output, shared, clock()) {
						return;
					}
				}
				Err(_) => return,
			},
			Poll::Wait(wait) => std::thread::sleep(wait.min(SLICE)),
			Poll::Finished => {
				shared.stops.fetch_add(1, Ordering::Relaxed);
			}
		}
	}
}

/// Carries out one command; `false` ends the thread.
fn apply(control: Control, sequencer: &mut Sequencer, output: &mut HeapProd<MidiEvent>, shared: &PlayerShared, now: Duration) -> bool {
	match control {
		Control::Load(song) => {
			if sequencer.stop() {
				shared.stops.fetch_add(1, Ordering::Relaxed);
			}
			sequencer.load(song);
		}
		Control::Play => sequencer.play(now),
		Control::Pause => sequencer.pause(now),
		Control::Stop => {
			if sequencer.stop() {
				shared.stops.fetch_add(1, Ordering::Relaxed);
			}
		}
		Control::Output(ring) => *output = ring,
		Control::Shutdown => return false,
	}
	publish(sequencer, shared, now);
	true
}

fn publish(sequencer: &Sequencer, shared: &PlayerShared, now: Duration) {
	let milliseconds = |seconds: f64| (seconds * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
	shared.transport.store(sequencer.transport() as u8, Ordering::Relaxed);
	shared.position_ms.store(milliseconds(sequencer.position(now)), Ordering::Relaxed);
	shared.duration_ms.store(milliseconds(sequencer.duration()), Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
