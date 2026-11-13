//! Tests of the MIDI file sequencer with made-up time, and of the player thread with a clock the test
//! moves by hand: nothing here depends on how fast the machine is.

use std::sync::Mutex;

use ringbuf::traits::{Consumer, Split};
use ringbuf::{HeapCons, HeapRb};

use super::*;
use crate::midi::smf::TimedEvent;

fn on(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOn {
		channel,
		key,
		velocity: 100,
	}
}

fn off(channel: u8, key: u8) -> MidiEvent {
	MidiEvent::NoteOff { channel, key }
}

fn pedal(channel: u8, value: u8) -> MidiEvent {
	MidiEvent::ControlChange {
		channel,
		controller: controller::SUSTAIN,
		value,
	}
}

fn song(events: &[(f64, MidiEvent)], duration: f64) -> Arc<Song> {
	Arc::new(Song {
		events: events
			.iter()
			.map(|(seconds, event)| TimedEvent {
				seconds: *seconds,
				event: *event,
			})
			.collect(),
		duration,
	})
}

/// Two notes a second apart, the first held for half a second; the song lasts three seconds.
fn two_notes() -> Arc<Song> {
	song(&[(0.0, on(0, 60)), (0.5, off(0, 60)), (1.0, on(1, 64)), (1.5, off(1, 64))], 3.0)
}

fn at(seconds: f64) -> Duration {
	Duration::from_secs_f64(seconds)
}

/// What one poll sent and what it said to wait for.
fn poll(sequencer: &mut Sequencer, seconds: f64) -> (Vec<MidiEvent>, Poll) {
	let mut sent = Vec::new();
	let verdict = sequencer.poll(at(seconds), &mut |event| {
		sent.push(event);
		true
	});
	(sent, verdict)
}

fn wait(seconds: f64) -> Poll {
	Poll::Wait(at(seconds))
}

/// `Poll::Wait` of about `seconds`: positions are sums of binary fractions.
fn assert_waits(verdict: Poll, seconds: f64) {
	match verdict {
		Poll::Wait(wait) => assert!((wait.as_secs_f64() - seconds).abs() < 1e-9, "waits {wait:?}, expected {seconds} s"),
		other => panic!("{other:?}, expected a wait of {seconds} s"),
	}
}

#[test]
fn messages_are_sent_when_they_are_due_and_in_the_order_of_the_song() {
	let mut sequencer = Sequencer::new();
	sequencer.load(two_notes());
	assert_eq!(sequencer.transport(), Transport::Stopped);
	assert_eq!(poll(&mut sequencer, 5.0), (Vec::new(), Poll::Idle), "loaded is not playing");

	// Play starts at clock time 10.
	sequencer.play(at(10.0));
	assert_eq!(sequencer.transport(), Transport::Playing);
	assert_eq!(poll(&mut sequencer, 10.0), (vec![on(0, 60)], wait(0.5)));
	let (sent, verdict) = poll(&mut sequencer, 10.25);
	assert!(sent.is_empty(), "nothing is due yet");
	assert_waits(verdict, 0.25);
	assert_eq!(poll(&mut sequencer, 10.5), (vec![off(0, 60)], wait(0.5)));
	// A late poll sends everything that has become due, in order.
	assert_eq!(poll(&mut sequencer, 11.75), (vec![on(1, 64), off(1, 64)], wait(1.25)));
	assert_eq!(sequencer.position(at(11.75)), 1.75);
	// The song goes on in silence to the end of its longest track, then stops by itself.
	assert_eq!(poll(&mut sequencer, 12.99).0, Vec::new());
	assert_eq!(sequencer.transport(), Transport::Playing);
	assert_eq!(poll(&mut sequencer, 13.0), (Vec::new(), Poll::Finished));
	assert_eq!(sequencer.transport(), Transport::Stopped);
	assert_eq!(sequencer.position(at(13.0)), 0.0);
	assert_eq!(poll(&mut sequencer, 14.0), (Vec::new(), Poll::Idle), "it finishes once");
}

#[test]
fn events_due_at_the_same_moment_keep_their_order() {
	let mut sequencer = Sequencer::new();
	let chord = [(0.25, on(0, 60)), (0.25, on(0, 64)), (0.25, on(0, 67)), (0.25, pedal(0, 127))];
	sequencer.load(song(&chord, 1.0));
	sequencer.play(at(0.0));
	assert_waits(poll(&mut sequencer, 0.0).1, 0.25);
	let (sent, _) = poll(&mut sequencer, 0.25);
	assert_eq!(sent, [on(0, 60), on(0, 64), on(0, 67), pedal(0, 127)]);
}

#[test]
fn pause_lets_the_notes_go_and_play_goes_on_from_the_same_place() {
	let mut sequencer = Sequencer::new();
	sequencer.load(two_notes());
	sequencer.play(at(0.0));
	poll(&mut sequencer, 0.0);
	// A quarter of a second in, the first note sounds.
	sequencer.pause(at(0.25));
	assert_eq!(sequencer.transport(), Transport::Paused);
	assert_eq!(
		poll(&mut sequencer, 0.25),
		(vec![off(0, 60)], Poll::Idle),
		"the sounding note is let go"
	);
	assert_eq!(sequencer.position(at(100.0)), 0.25, "time does not move while paused");
	assert_eq!(poll(&mut sequencer, 100.0), (Vec::new(), Poll::Idle));

	// Much later play goes on: the rest of the song comes as far from now as it was from the pause.
	sequencer.play(at(100.0));
	assert_eq!(sequencer.position(at(100.0)), 0.25);
	// The note that was cut is not started again; its own Note Off is still sent and harms nothing.
	let (sent, verdict) = poll(&mut sequencer, 100.0);
	assert!(sent.is_empty());
	assert_waits(verdict, 0.25);
	assert_eq!(poll(&mut sequencer, 100.25).0, [off(0, 60)]);
	assert_eq!(poll(&mut sequencer, 100.75).0, [on(1, 64)]);
	assert_eq!(sequencer.position(at(100.75)), 1.0);

	// Pausing while stopped, or twice, changes nothing.
	sequencer.pause(at(101.0));
	sequencer.pause(at(105.0));
	assert_eq!(sequencer.position(at(200.0)), 1.25);
	sequencer.stop();
	sequencer.pause(at(201.0));
	assert_eq!(sequencer.transport(), Transport::Stopped);
}

#[test]
fn stop_lets_the_notes_and_pedals_go_and_returns_to_the_start() {
	let held = [
		(0.0, on(0, 60)),
		(0.0, on(0, 64)),
		(0.0, pedal(0, 127)),
		(0.0, on(9, 36)),
		(0.0, pedal(3, 64)),
		(0.0, pedal(4, 63)),
		(0.5, off(0, 64)),
		(2.0, off(0, 60)),
	];
	let mut sequencer = Sequencer::new();
	sequencer.load(song(&held, 2.0));
	sequencer.play(at(0.0));
	poll(&mut sequencer, 0.6);
	assert!(sequencer.stop(), "there was something to stop");
	assert_eq!(sequencer.transport(), Transport::Stopped);
	assert_eq!(sequencer.position(at(0.6)), 0.0);
	// Channel by channel: the pedal up, then the notes still on. The note that ended and the pedal
	// that was never down get nothing.
	assert_eq!(
		poll(&mut sequencer, 0.6),
		(vec![pedal(0, 0), off(0, 60), pedal(3, 0), off(9, 36)], Poll::Idle)
	);
	assert_eq!(poll(&mut sequencer, 0.7), (Vec::new(), Poll::Idle), "they are let go once");
	assert!(!sequencer.stop(), "stopping what is stopped is nothing");

	// Play starts from the beginning again.
	sequencer.play(at(50.0));
	assert_eq!(poll(&mut sequencer, 50.0).0.len(), 6);
}

#[test]
fn without_a_song_and_while_playing_play_changes_nothing() {
	let mut sequencer = Sequencer::new();
	sequencer.play(at(1.0));
	assert_eq!(sequencer.transport(), Transport::Stopped);
	assert_eq!((sequencer.duration(), sequencer.position(at(9.0))), (0.0, 0.0));
	assert_eq!(poll(&mut sequencer, 2.0), (Vec::new(), Poll::Idle));

	sequencer.load(two_notes());
	assert_eq!(sequencer.duration(), 3.0);
	sequencer.play(at(0.0));
	poll(&mut sequencer, 0.6);
	// A second play in the middle does not restart the song.
	sequencer.play(at(0.7));
	assert_eq!(sequencer.position(at(0.7)), 0.7);
	assert_eq!(poll(&mut sequencer, 1.0).0, [on(1, 64)]);
}

#[test]
fn loading_another_song_stops_the_one_that_plays() {
	let mut sequencer = Sequencer::new();
	sequencer.load(two_notes());
	sequencer.play(at(0.0));
	poll(&mut sequencer, 0.1);
	sequencer.load(song(&[(0.0, on(5, 72))], 0.5));
	assert_eq!(sequencer.transport(), Transport::Stopped);
	assert_eq!(sequencer.duration(), 0.5);
	assert_eq!(
		poll(&mut sequencer, 0.2),
		(vec![off(0, 60)], Poll::Idle),
		"the old song's note is let go"
	);
	sequencer.play(at(1.0));
	assert_eq!(poll(&mut sequencer, 1.0).0, [on(5, 72)]);
}

#[test]
fn a_song_that_ends_with_notes_on_lets_them_go() {
	let mut sequencer = Sequencer::new();
	sequencer.load(song(&[(0.0, on(0, 60)), (0.0, pedal(1, 127))], 1.0));
	sequencer.play(at(0.0));
	assert_eq!(poll(&mut sequencer, 0.0).0, [on(0, 60), pedal(1, 127)]);
	assert_eq!(poll(&mut sequencer, 1.0), (vec![off(0, 60), pedal(1, 0)], Poll::Finished));
}

#[test]
fn a_message_the_ring_has_no_room_for_is_offered_again_and_nothing_is_lost() {
	let mut sequencer = Sequencer::new();
	sequencer.load(song(&[(0.0, on(0, 60)), (0.0, on(0, 62)), (0.0, on(0, 64))], 5.0));
	sequencer.play(at(0.0));
	// The ring takes one message and is full.
	let mut taken = Vec::new();
	let mut room = 1;
	let verdict = sequencer.poll(at(0.0), &mut |event| {
		if room == 0 {
			return false;
		}
		room -= 1;
		taken.push(event);
		true
	});
	assert_eq!(taken, [on(0, 60)]);
	assert_eq!(verdict, Poll::Wait(RETRY));
	assert_eq!(poll(&mut sequencer, 0.001).0, [on(0, 62), on(0, 64)], "the rest follows in order");

	// The same when the notes are let go: what did not fit is still held and comes with the next poll.
	sequencer.stop();
	let mut room = 2;
	let mut taken = Vec::new();
	let verdict = sequencer.poll(at(0.002), &mut |event| {
		if room == 0 {
			return false;
		}
		room -= 1;
		taken.push(event);
		true
	});
	assert_eq!((taken, verdict), (vec![off(0, 60), off(0, 62)], Poll::Wait(RETRY)));
	assert_eq!(poll(&mut sequencer, 0.003), (vec![off(0, 64)], Poll::Idle));
}

/// A clock the test sets, and the far end of the ring the player sends into.
struct Rig {
	player: Player,
	clock: Arc<Mutex<Duration>>,
	ring: HeapCons<MidiEvent>,
}

fn rig() -> Rig {
	let (output, ring) = HeapRb::<MidiEvent>::new(64).split();
	let clock = Arc::new(Mutex::new(Duration::ZERO));
	let read = Arc::clone(&clock);
	let player = Player::spawn_with(output, move || *read.lock().expect("clock"));
	Rig { player, clock, ring }
}

impl Rig {
	fn set_time(&self, seconds: f64) {
		*self.clock.lock().expect("clock") = at(seconds);
	}

	/// Waits until `count` messages have arrived and returns them; the thread is given a generous two
	/// seconds of real time, which it needs only if the machine is very busy.
	fn received(&mut self, count: usize) -> Vec<MidiEvent> {
		drain(&mut self.ring, count)
	}

	/// Waits until the player reports `transport`.
	fn wait_for(&self, transport: Transport) -> PlayerStatus {
		wait_until(|| Some(self.player.status()).filter(|status| status.transport == transport))
	}
}

fn drain(ring: &mut HeapCons<MidiEvent>, count: usize) -> Vec<MidiEvent> {
	let mut events = Vec::new();
	wait_until(|| {
		while let Some(event) = ring.try_pop() {
			events.push(event);
		}
		(events.len() >= count).then_some(())
	});
	events
}

fn wait_until<T>(mut check: impl FnMut() -> Option<T>) -> T {
	let deadline = Instant::now() + Duration::from_secs(2);
	loop {
		if let Some(found) = check() {
			return found;
		}
		assert!(Instant::now() < deadline, "the player thread did not get there in two seconds");
		std::thread::yield_now();
	}
}

#[test]
fn the_player_thread_sends_the_song_into_its_ring_as_its_clock_moves() {
	let mut rig = rig();
	assert_eq!(rig.player.status().transport, Transport::Stopped);
	rig.player.load(two_notes());
	rig.player.play();
	assert_eq!(rig.received(1), [on(0, 60)]);
	let status = rig.wait_for(Transport::Playing);
	assert_eq!(status.duration, Duration::from_secs(3));

	rig.set_time(1.2);
	assert_eq!(rig.received(2), [off(0, 60), on(1, 64)]);
	wait_until(|| (rig.player.status().position == Duration::from_millis(1200)).then_some(()));
	assert_eq!(rig.player.stops(), 0);

	// At the end the player stops by itself and counts the stop.
	rig.set_time(3.0);
	assert_eq!(rig.received(1), [off(1, 64)]);
	let status = rig.wait_for(Transport::Stopped);
	assert_eq!(status.position, Duration::ZERO);
	wait_until(|| (rig.player.stops() == 1).then_some(()));
}

#[test]
fn pause_and_stop_reach_the_thread_and_only_stops_are_counted() {
	let mut rig = rig();
	rig.player.load(two_notes());
	rig.player.play();
	assert_eq!(rig.received(1), [on(0, 60)]);
	rig.set_time(0.2);
	rig.player.pause();
	assert_eq!(rig.received(1), [off(0, 60)]);
	let status = rig.wait_for(Transport::Paused);
	assert_eq!(status.position, Duration::from_millis(200));
	assert_eq!(rig.player.stops(), 0, "a pause is not a stop");

	rig.player.play();
	rig.wait_for(Transport::Playing);
	rig.set_time(0.9);
	// Half a second was left to the second note when the song was paused at 0.2 with the clock at 0.2.
	assert_eq!(rig.received(1), [off(0, 60)]);
	rig.player.stop();
	rig.wait_for(Transport::Stopped);
	wait_until(|| (rig.player.stops() == 1).then_some(()));
	// Stopping again counts nothing; loading over a stopped song neither.
	rig.player.stop();
	rig.player.load(two_notes());
	rig.player.play();
	rig.wait_for(Transport::Playing);
	assert_eq!(rig.player.stops(), 1);
	// Loading over a playing song stops it, and that counts.
	rig.player.load(two_notes());
	rig.wait_for(Transport::Stopped);
	wait_until(|| (rig.player.stops() == 2).then_some(()));
}

#[test]
fn a_song_goes_on_into_the_ring_of_a_reopened_stream() {
	let mut rig = rig();
	rig.player.load(two_notes());
	rig.player.play();
	assert_eq!(rig.received(1), [on(0, 60)]);

	// The audio stream is reopened: the player gets the ring of the new one.
	let (output, mut new_ring) = HeapRb::<MidiEvent>::new(64).split();
	rig.player.set_output(output);
	rig.set_time(1.6);
	assert_eq!(drain(&mut new_ring, 3), [off(0, 60), on(1, 64), off(1, 64)]);
	assert_eq!(rig.ring.try_pop(), None, "nothing more goes into the old ring");
	assert_eq!(
		rig.player.status().transport,
		Transport::Playing,
		"the song did not stop for the switch"
	);
}

#[test]
fn dropping_the_player_ends_its_thread_whatever_it_was_doing() {
	// Idle, playing with a long wait ahead, and with a ring nobody empties: each ends promptly.
	drop(rig().player);

	let playing = rig();
	playing.player.load(song(&[(1000.0, on(0, 60))], 2000.0));
	playing.player.play();
	playing.wait_for(Transport::Playing);
	drop(playing.player);

	let (output, _ring) = HeapRb::<MidiEvent>::new(1).split();
	let clock = Arc::new(Mutex::new(Duration::ZERO));
	let read = Arc::clone(&clock);
	let stuck = Player::spawn_with(output, move || *read.lock().expect("clock"));
	stuck.load(song(&[(0.0, on(0, 60)), (0.0, on(0, 62)), (0.0, on(0, 64))], 1.0));
	stuck.play();
	wait_until(|| (stuck.status().transport == Transport::Playing).then_some(()));
	drop(stuck);
}
