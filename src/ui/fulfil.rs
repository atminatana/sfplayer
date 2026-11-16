//! Applying the requests of the UI to the session: the one place of the UI that knows `Session`.

use super::app::{App, Modal, Request};
use super::dialog::{Choice, PresetRequest};
use super::effects_dialog::EffectsDialog;
use super::path_input::{InputKind, PathRequest};
use crate::session::{EffectsAction, Session};

/// Applies a UI request to the session and the app (dialog contents come from the session).
pub fn fulfil(request: Request, app: &mut App, session: &mut Session) {
	match request {
		Request::Mixer(action) => session.apply_mixer(action),
		Request::OpenPresetDialog => {
			if let Some(loaded) = &session.bank {
				app.preset_request = Some(PresetRequest {
					presets: loaded.bank.presets().to_vec(),
					current: session.preset_for(),
				});
			}
		}
		Request::OpenRecordDialog => {
			let record = &session.config.record;
			app.path_request = Some(PathRequest {
				kind: InputKind::RecordDir,
				initial: record
					.dir
					.as_ref()
					.map(|dir| dir.to_string_lossy().into_owned())
					.unwrap_or_default(),
				history: record.recent_dirs.clone(),
			});
		}
		Request::OpenEffectsDialog => app.open_modal(Modal::Effects(EffectsDialog::new(session.effects()))),
		Request::OpenSf2Dialog => {
			let initial = session
				.bank
				.as_ref()
				.map(|loaded| loaded.path.to_string_lossy().into_owned())
				.unwrap_or_default();
			app.path_request = Some(PathRequest {
				kind: InputKind::Sf2File,
				initial,
				history: session.config.synth.recent_banks.clone(),
			});
		}
		Request::OpenMidiFileDialog => {
			let source = &session.config.source;
			app.path_request = Some(PathRequest {
				kind: InputKind::MidiFile,
				initial: source
					.midi_file
					.as_ref()
					.map(|path| path.to_string_lossy().into_owned())
					.unwrap_or_default(),
				history: source.recent_midi_files.clone(),
			});
		}
		Request::FilePlay => session.file_play(),
		Request::FilePause => session.file_pause(),
		Request::FilePlayPause => session.file_play_pause(),
		Request::FileStop => session.file_stop(),
		Request::Apply(Choice::ConnectMidi(index)) => {
			session.connect_midi(index);
			session.save_config();
		}
		Request::Apply(Choice::OpenAudio(index)) => {
			session.reopen_audio(index);
			session.save_config();
		}
		Request::Apply(Choice::SelectPreset { bank, program }) => {
			session.select_preset(bank, program);
			session.save_config();
		}
		Request::SetRecordFormat(format) => {
			session.set_record_format(format);
			session.save_config();
		}
		// The engine changes under the keys: the notes the text keyboard holds end with the old one.
		Request::SetEngine(kind) => {
			app.keyboard.clear();
			session.set_engine(kind);
			session.save_config();
		}
		Request::ToggleEngine => {
			app.keyboard.clear();
			session.set_engine(session.config.synth.engine.other());
			session.save_config();
		}
		// The timbre is written with the rest of the config when the program closes, like the master
		// volume: these are stepped many times in a row.
		Request::SetWave(wave) => session.set_wave(wave),
		Request::SetAdsrPreset(preset) => session.set_adsr_preset(preset),
		Request::AdjustAdsr(field, steps) => session.adjust_adsr(field, steps),
		Request::Rescan => session.rescan(),
		Request::StartRecord => session.start_recording(),
		Request::CancelRecord => session.cancel_recording(),
		Request::StopRecord | Request::StopRecordingAndQuit => session.stop_recording(),
		Request::Panic => session.panic(),
		Request::Transpose(delta) => session.adjust_transpose(delta),
		Request::Effects(action) => session.apply_effects(action),
		Request::SaveEffects(settings) => {
			session.apply_effects(EffectsAction::Set(settings));
			session.save_config();
		}
		Request::Notes(events) => {
			tracing::debug!(?events, "notes to the command ring");
			session.send_notes(&events);
		}
	}
}

#[cfg(test)]
mod tests;
