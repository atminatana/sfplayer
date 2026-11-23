//! WASAPI with the device taken for this program alone (spec v1.4, E-01…E-08; ADR 0019).
//!
//! cpal opens WASAPI in shared mode only, where a block reaches the device tens of milliseconds
//! after it is rendered. Here the device is opened exclusively and asks for a block once a period,
//! a few milliseconds long. The stream is a thread of its own: it opens the device, waits for the
//! event of the device, renders a block and writes it as the integer PCM the device takes.
//!
//! Everything that talks to COM is in this file and happens on that thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE, S_OK, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
	AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED, AUDCLNT_SHAREMODE_EXCLUSIVE,
	AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM,
	WAVEFORMATEX,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Threading::{
	AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, WaitForSingleObject,
};
use windows::core::{HSTRING, PCWSTR, w};

use super::pcm::{PcmBlock, PcmFormat};
use super::{AudioShared, BlockRenderer};

/// Units of 100 ns in a second: what WASAPI measures periods in.
const HNS_PER_SECOND: u64 = 10_000_000;
/// How long the thread waits for the device before it looks whether it was told to stop. The device
/// wakes it once a period, far more often: this matters only when the device has gone silent.
const STOP_CHECK_MS: u32 = 200;
/// Channel counts a device is asked for, best first; the render core mixes its stereo down to one.
const CHANNELS: [u16; 2] = [2, 1];

/// What the stream is asked for.
#[derive(Debug, Clone, Copy)]
pub struct Request {
	pub sample_rate: u32,
	/// Frames of a period; the device's shortest period when that is longer.
	pub buffer_frames: u32,
}

/// What the device agreed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opened {
	pub format: PcmFormat,
	pub channels: u16,
	/// Frames the device asks for at a time.
	pub period_frames: u32,
}

/// A running exclusive stream; dropping it stops playback and gives the device back to the system.
pub struct ExclusiveStream {
	stop: Arc<AtomicBool>,
	thread: Option<JoinHandle<()>>,
}

impl Drop for ExclusiveStream {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Relaxed);
		if let Some(thread) = self.thread.take() {
			// A panic of the audio thread is already in the log (the panic hook of `main`); there is
			// nothing more to do about it while closing the stream.
			let _ = thread.join();
		}
	}
}

/// Opens the endpoint `endpoint_id` exclusively and starts playing what `renderer` renders.
/// The error says why the device could not be opened, in words fit for the output row.
pub fn open(
	endpoint_id: &str,
	request: Request,
	renderer: BlockRenderer,
	shared: Arc<AudioShared>,
) -> Result<(ExclusiveStream, Opened), String> {
	let stop = Arc::new(AtomicBool::new(false));
	let (opened_tx, opened_rx) = mpsc::sync_channel(1);
	let endpoint_id = endpoint_id.to_owned();
	let thread_stop = Arc::clone(&stop);
	let thread = std::thread::Builder::new()
		.name("audio-exclusive".to_owned())
		.spawn(move || run(&endpoint_id, request, renderer, &shared, &thread_stop, &opened_tx))
		.map_err(|error| format!("cannot start the audio thread: {error}"))?;
	let stream = ExclusiveStream {
		stop,
		thread: Some(thread),
	};
	// A stream that did not open is dropped here: its thread has ended or is about to.
	match opened_rx.recv() {
		Ok(Ok(opened)) => Ok((stream, opened)),
		Ok(Err(reason)) => Err(reason),
		Err(_) => Err("the audio thread ended before the device was opened".to_owned()),
	}
}

/// The body of the audio thread: opens the device, reports how that went, plays until told to stop.
fn run(
	endpoint_id: &str,
	request: Request,
	mut renderer: BlockRenderer,
	shared: &AudioShared,
	stop: &AtomicBool,
	opened_tx: &SyncSender<Result<Opened, String>>,
) {
	// SAFETY: plain initialization of COM for this thread; paired with `CoUninitialize` below.
	let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
	if com.is_err() {
		// The opener is waiting for this message; if it has gone, there is nobody to tell.
		let _ = opened_tx.send(Err(format!("cannot initialize COM: {com:?}")));
		return;
	}
	// The device is in a scope of its own: its COM objects must be released before COM is.
	{
		match Device::open(endpoint_id, request) {
			Ok(device) => {
				// An opener that has gone no longer wants the stream.
				if opened_tx.send(Ok(device.opened)).is_ok()
					&& let Err(error) = device.play(&mut renderer, shared, stop)
				{
					tracing::warn!(error = %describe(&error), "exclusive audio stream stopped");
				}
			}
			Err(reason) => {
				let _ = opened_tx.send(Err(reason));
			}
		}
	}
	// SAFETY: pairs with the successful `CoInitializeEx` above; no COM object of this thread is alive.
	unsafe { CoUninitialize() };
}

/// The event the device signals when it wants the next block.
struct Event(HANDLE);

impl Event {
	fn new() -> windows::core::Result<Self> {
		// SAFETY: an unnamed auto-reset event with default security; the handle is owned by `Self`.
		unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.map(Self)
	}
}

impl Drop for Event {
	fn drop(&mut self) {
		// SAFETY: the handle came from `CreateEventW` and is closed once. A failure to close it leaks
		// one handle and cannot be acted upon.
		let _ = unsafe { CloseHandle(self.0) };
	}
}

/// An endpoint opened exclusively and ready to start.
struct Device {
	client: IAudioClient,
	render: IAudioRenderClient,
	event: Event,
	sample_rate: u32,
	opened: Opened,
}

impl Device {
	fn open(endpoint_id: &str, request: Request) -> Result<Self, String> {
		let sample_rate = request.sample_rate;
		// SAFETY: COM is initialized on this thread; every pointer passed below is to a live local,
		// and the interfaces are used as their documentation says: a client is initialized once.
		unsafe {
			let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| describe(&e))?;
			let endpoint = enumerator.GetDevice(&HSTRING::from(endpoint_id)).map_err(|e| describe(&e))?;
			let activate = || endpoint.Activate::<IAudioClient>(CLSCTX_ALL, None).map_err(|e| describe(&e));
			let client = activate()?;
			let (wave, format, channels) = accepted_format(&client, sample_rate)
				.ok_or_else(|| format!("the device takes neither 24 nor 16 bit PCM at {sample_rate} Hz in exclusive mode"))?;
			let mut shortest = 0i64;
			client.GetDevicePeriod(None, Some(&mut shortest)).map_err(|e| describe(&e))?;
			let period = hns_of(request.buffer_frames, sample_rate).max(shortest);
			let initialize = |client: &IAudioClient, period: i64| {
				client.Initialize(
					AUDCLNT_SHAREMODE_EXCLUSIVE,
					AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
					period,
					period,
					&wave,
					None,
				)
			};
			let client = match initialize(&client, period) {
				Ok(()) => client,
				// The device wants a whole number of its own blocks and has said how many frames that
				// is. A client that failed to initialize cannot be tried again: a new one is made.
				Err(error) if error.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
					let aligned = hns_of(client.GetBufferSize().map_err(|e| describe(&e))?, sample_rate);
					let client = activate()?;
					initialize(&client, aligned).map_err(|e| describe(&e))?;
					client
				}
				Err(error) => return Err(describe(&error)),
			};
			let period_frames = client.GetBufferSize().map_err(|e| describe(&e))?;
			let event = Event::new().map_err(|e| describe(&e))?;
			client.SetEventHandle(event.0).map_err(|e| describe(&e))?;
			let render: IAudioRenderClient = client.GetService().map_err(|e| describe(&e))?;
			Ok(Self {
				client,
				render,
				event,
				sample_rate,
				opened: Opened {
					format,
					channels,
					period_frames,
				},
			})
		}
	}

	/// Plays until `stop` is set or the device fails. Past the set-up, the loop allocates nothing,
	/// takes no lock and calls nothing but the device (N-03).
	fn play(&self, renderer: &mut BlockRenderer, shared: &AudioShared, stop: &AtomicBool) -> windows::core::Result<()> {
		let Opened {
			format,
			channels,
			period_frames,
		} = self.opened;
		let mut block = PcmBlock::new(format, usize::from(channels), period_frames as usize);
		let period_bytes = period_frames as usize * block.frame_bytes();

		// The scheduler's class for audio threads; without it the thread is an ordinary one and a
		// busy system makes it late.
		let mut task_index = 0u32;
		// SAFETY: a documented call with a static task name and a pointer to a live local.
		let priority = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index) };
		if let Err(error) = &priority {
			tracing::warn!(error = %describe(error), "the audio thread keeps its normal priority");
		}

		// SAFETY: `GetBuffer` gives `period_frames` frames of `frame_bytes` bytes each, valid until
		// `ReleaseBuffer`; exactly that many bytes are written.
		let played = unsafe {
			// The device plays a buffer before it asks for the next one: the first is silence.
			let first = self.render.GetBuffer(period_frames)?;
			std::ptr::write_bytes(first, 0, period_bytes);
			self.render.ReleaseBuffer(period_frames, 0)?;
			self.client.Start()?;
			let played = self.pump(&mut block, period_bytes, renderer, shared, stop);
			// The stream is over either way; a failure to stop a device that is already gone is
			// what `played` reports.
			let _ = self.client.Stop();
			played
		};
		if let Ok(handle) = priority {
			// SAFETY: the handle came from `AvSetMmThreadCharacteristicsW` on this thread. The thread
			// ends right after, so a failure to revert changes nothing.
			let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
		}
		played
	}

	/// One block per wake-up of the device.
	///
	/// # Safety
	/// The stream must be started, and `period_bytes` must be the size of the device buffer.
	unsafe fn pump(
		&self,
		block: &mut PcmBlock,
		period_bytes: usize,
		renderer: &mut BlockRenderer,
		shared: &AudioShared,
		stop: &AtomicBool,
	) -> windows::core::Result<()> {
		let period_frames = self.opened.period_frames;
		let period = Duration::from_secs_f64(f64::from(period_frames) / f64::from(self.sample_rate));
		// A wake-up this much after the previous one means the device played a block twice.
		let late = period + period / 2;
		// The first wake-up has nothing to be late after: a device may take its time to start.
		let mut woken: Option<Instant> = None;
		while !stop.load(Ordering::Relaxed) {
			// SAFETY: the event handle is alive as long as `self`.
			match unsafe { WaitForSingleObject(self.event.0, STOP_CHECK_MS) } {
				WAIT_OBJECT_0 => {}
				WAIT_TIMEOUT => continue,
				_ => return Err(windows::core::Error::from_thread()),
			}
			let now = Instant::now();
			if woken.is_some_and(|before| now.duration_since(before) > late) {
				shared.record_xrun();
			}
			woken = Some(now);
			// SAFETY: in exclusive event mode the device hands out its whole buffer, `period_bytes`
			// long, valid until `ReleaseBuffer`; the slice does not outlive that call.
			unsafe {
				let buffer = self.render.GetBuffer(period_frames)?;
				block.render(renderer, std::slice::from_raw_parts_mut(buffer, period_bytes));
				self.render.ReleaseBuffer(period_frames, 0)?;
			}
		}
		Ok(())
	}
}

/// The first format the device takes exclusively at `sample_rate`: stereo before mono, 24 bits
/// before 16 (E-01).
///
/// # Safety
/// COM must be initialized on this thread.
unsafe fn accepted_format(client: &IAudioClient, sample_rate: u32) -> Option<(WAVEFORMATEX, PcmFormat, u16)> {
	for channels in CHANNELS {
		for format in PcmFormat::PREFERRED {
			let wave = wave_format(format, channels, sample_rate);
			// SAFETY: a pointer to a live local; no closest match is asked for in exclusive mode.
			if unsafe { client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &wave, None) } == S_OK {
				return Some((wave, format, channels));
			}
		}
	}
	None
}

/// Plain integer PCM as WASAPI describes it.
fn wave_format(format: PcmFormat, channels: u16, sample_rate: u32) -> WAVEFORMATEX {
	let frame_bytes = channels * (format.bits() / 8);
	WAVEFORMATEX {
		wFormatTag: WAVE_FORMAT_PCM as u16,
		nChannels: channels,
		nSamplesPerSec: sample_rate,
		nAvgBytesPerSec: sample_rate * u32::from(frame_bytes),
		nBlockAlign: frame_bytes,
		wBitsPerSample: format.bits(),
		cbSize: 0,
	}
}

/// `frames` at `sample_rate` as a duration in units of 100 ns, rounded to the nearest.
fn hns_of(frames: u32, sample_rate: u32) -> i64 {
	let rate = u64::from(sample_rate.max(1));
	((u64::from(frames) * HNS_PER_SECOND + rate / 2) / rate) as i64
}

/// Why a call failed, in words fit for the output row: the two refusals a user can do something
/// about are named plainly, the rest carry the system's own text and code.
fn describe(error: &windows::core::Error) -> String {
	match error.code() {
		code if code == AUDCLNT_E_DEVICE_IN_USE => "the device is in use by another program".to_owned(),
		code if code == AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => "exclusive mode is switched off in the properties of the device".to_owned(),
		code => format!("{} ({:#010X})", error.message(), code.0),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_period_in_frames_is_a_duration_in_units_of_100_ns() {
		assert_eq!(hns_of(480, 48_000), 100_000, "10 ms");
		assert_eq!(hns_of(256, 48_000), 53_333, "5.33 ms, rounded");
		assert_eq!(hns_of(441, 44_100), 100_000);
		assert_eq!(hns_of(0, 48_000), 0);
		assert_eq!(hns_of(256, 0), 2_560_000_000, "a zero rate does not divide by zero");
	}

	#[test]
	fn a_wave_format_describes_plain_integer_pcm() {
		let wave = wave_format(PcmFormat::I24, 2, 48_000);
		let (tag, channels, rate, bytes_per_second, frame_bytes, bits) = (
			wave.wFormatTag,
			wave.nChannels,
			wave.nSamplesPerSec,
			wave.nAvgBytesPerSec,
			wave.nBlockAlign,
			wave.wBitsPerSample,
		);
		assert_eq!(
			(tag, channels, rate, bytes_per_second, frame_bytes, bits),
			(1, 2, 48_000, 288_000, 6, 24)
		);
		let mono = wave_format(PcmFormat::I16, 1, 16_000);
		let (frame_bytes, bytes_per_second) = (mono.nBlockAlign, mono.nAvgBytesPerSec);
		assert_eq!((frame_bytes, bytes_per_second), (2, 32_000));
	}

	#[test]
	fn the_refusals_a_user_can_act_on_are_named_plainly() {
		let busy = windows::core::Error::from_hresult(AUDCLNT_E_DEVICE_IN_USE);
		assert_eq!(describe(&busy), "the device is in use by another program");
		let forbidden = windows::core::Error::from_hresult(AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED);
		assert!(describe(&forbidden).contains("switched off in the properties"));
		let other = windows::core::Error::from_hresult(AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED);
		assert!(describe(&other).ends_with("(0x88890019)"), "{}", describe(&other));
	}
}
