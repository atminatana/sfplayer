# sf2_player

[English](README.md) | [Русский](README.ru.md)

Console cross-platform SoundFont synthesizer in Rust: notes from a MIDI keyboard (or from the two bottom rows of a regular keyboard) are played through the chosen audio output with a SoundFont 2 (.sf2) bank. The terminal UI lists the chosen devices, bank, preset, effects and recording settings on F-keys, shows the output level and the master volume, and keeps the counters in the bottom border; a session can be recorded to FLAC or Ogg Vorbis. Windows, Linux, macOS; the interface is in English or Russian.

## Installation

**Prebuilt binaries** — the Releases tab: archives for Windows x86_64, Linux x86_64, macOS arm64 and x86_64. Unpack and run `sf2_player`.

**Build from source** (Rust stable, edition 2024):

```bash
cargo build --release
./target/release/sf2_player --sf2 path/to/bank.sf2
```

What the machine needs:

| OS | Dependencies |
|---|---|
| Windows 10+ | Rust toolchain with MSVC (a C compiler is needed for Ogg Vorbis; it ships with Build Tools) |
| Linux | `libasound2-dev` (ALSA, for cpal), `gcc` or `clang`, `pkg-config`; PipeWire and PulseAudio work through the ALSA compatibility layer |
| macOS 14+ | Xcode Command Line Tools |

SF2 banks are not shipped with the program. Free General MIDI banks: GeneralUser GS, FluidR3, TimGM6mb.

## Running

```
sf2_player [--sf2 PATH] [--midi-in NAME|INDEX] [--audio-out NAME|INDEX]
           [--buffer FRAMES] [--sample-rate HZ] [--config PATH]
           [--log-level LEVEL] [--ascii] [--list-devices]
```

- `--list-devices` prints MIDI inputs and audio outputs with their indices and exits.
- Devices whose configuration cannot be read (an unsupported ALSA hint, a busy device) are skipped with a warning in the log instead of breaking the whole list.
- Devices are selected by index or by a case-insensitive substring of the name; if a device is not found, the system default is used (the first port for MIDI) and a warning appears in the status line. If a device is found but cannot be opened (busy, unsupported configuration), the program runs without sound, the status line shows the cause, and another device is chosen with F3.
- The default buffer is 256 frames; if the device does not accept it, the device default is used.
- Command-line values take precedence over the config file.

Without `--sf2` the program starts silent: a bank is chosen with F4.

## Hotkeys

There are no letter commands: the two bottom keyboard rows play notes. All commands live on F-keys, Ctrl/Alt chords and non-letter keys. The table below is generated from the same binding table as the F1 help (the `readme_keys` test keeps them in sync).

<!-- keys:start -->
| Keys | Action |
|---|---|
| F1 | This help |
| F2 | Choose MIDI input |
| F3 | Choose audio output |
| F4 | Open SoundFont file |
| F5 | Choose the preset for all channels except 10 |
| F6 | Effects: reverb / chorus on, off and send levels |
| F7 | Recording settings: folder and format |
| Home | Start recording |
| End | Stop recording |
| F8 | Rescan MIDI ports |
| F9 / Space | Panic: all notes off + all sound off |
| F10 / Ctrl+C | Quit |
| Ctrl+↓ | Text keyboard octave −1 |
| Ctrl+↑ | Text keyboard octave +1 |
| Ctrl+← | Text keyboard velocity −10 |
| Ctrl+→ | Text keyboard velocity +10 |
| Esc | Release all text-keyboard notes |
| [ | Transpose −1 semitone (down to −12) |
| ] | Transpose +1 semitone (up to +12) |
| Alt+R | Reverb on / off (level: F6 or config synth.reverb_level) |
| Alt+C | Chorus on / off (level: F6 or config synth.chorus_level) |
| + / = | Master volume +1 % |
| - | Master volume −1 % |
| Shift++ | Master volume +5 % |
| Shift+_ / _ | Master volume −5 % |
| Ctrl+U | Mute master |
| Z X C V B N M , . / | White keys C–E from the base octave (text keyboard) |
| S D G H J L ; | Black keys C# D# F# G# A# C# D# (text keyboard) |
<!-- keys:end -->

While a dialog is open, input goes to the dialog: ↑↓, PgUp/PgDn, Home/End move through lists and fields there, Enter applies, Esc cancels. On the main screen Home and End start and stop recording.

### Text keyboard

```
  S  D     G  H  J     L  ;        ← black keys (C#4 D#4 F#4 G#4 A#4 C#5 D#5)
 Z  X  C  V  B  N  M  ,  .  /     ← white keys (C4 D4 E4 F4 G4 A4 B4 C5 D5 E5)
```

Notes play on MIDI channel 1 with the F5 preset. How a note is released depends on the terminal:

- **release** — the terminal reports key release: the Windows console (including Windows Terminal) and terminals with the kitty keyboard protocol (kitty, WezTerm, foot, Ghostty). The note sounds while the key is held.
- **gate** — other terminals: the note sounds for `keyboard.gate_ms` milliseconds (300 by default) or until the same key is pressed again.

The mode is detected at startup and shown in the status line as `kbd: release` or `kbd: gate 300ms`. If notes "stick" in your terminal, set `force_gate = true` in the `[keyboard]` section.

## Configuration

A TOML file is created on the first exit and stores the selected devices, the bank and the last 10 banks, master volume, effects and recording format:

| OS | Path |
|---|---|
| Windows | `%APPDATA%\sf2_player\config\config.toml` |
| Linux | `~/.config/sf2_player/config.toml` |
| macOS | `~/Library/Application Support/sf2_player/config.toml` |

The log is written to the local data directory (`%LOCALAPPDATA%\sf2_player\data\sf2_player.log`, `~/.local/share/sf2_player/`, `~/Library/Application Support/sf2_player/`); the level is set by `--log-level`. Each start removes the previous run's log, so the file always covers a single session.

All fields are optional; unknown keys are ignored.

```toml
[midi]
input = "KeyStep"          # name substring or index
omni_channel = 1           # route all incoming notes to channel 1 (1–16); remove to disable

[audio]
output = "Speakers"
buffer_frames = 256
sample_rate = 48000        # remove — use the device rate

[synth]
sf2 = "C:/banks/GeneralUser GS.sf2"
preset = [0, 0]            # F5 choice as [bank, program]; remove — the bank's first preset
reverb = true
chorus = false
reverb_level = 40          # CC 91 send, 0–127
chorus_level = 20          # CC 93 send, 0–127

[mixer]
master = 80                # 0–100 %

[keyboard]
white = "zxcvbnm,./"       # white keys from C
black = "sdghjl;"          # black keys C# D# F# G# A# C# D#
base_octave = 4
velocity = 100
gate_ms = 300
force_gate = false
octave_up = "F11"          # alternatives for terminals without Ctrl+arrows
velocity_down = "Alt+Down"

[keys]                     # command = chord; several — comma-separated
quit = "F10, Ctrl+Q"
panic = "F9"
rescan = "Alt+F3"

[ui]
lang = "en"                # "ru" | "en"; remove — system language
ascii = false              # true — indicators without Unicode block characters

[record]
dir = "D:/Music/sf2"       # remove — the Music folder/sf2_player
format = "flac"            # "flac" | "ogg_vorbis"
vorbis_quality = 0.5       # 0.0–1.0
```

Command names for `[keys]`: `help`, `quit`, `focus_next`, `focus_previous`, `midi_dialog`, `audio_dialog`, `sf2_dialog`, `preset_dialog`, `effects_dialog`, `record_dialog`, `rescan`, `record_start`, `record_stop`, `panic`, `octave_up`, `octave_down`, `velocity_up`, `velocity_down`, `notes_off`, `transpose_up`, `transpose_down`, `toggle_reverb`, `toggle_chorus`, `master_up`, `master_down`, `master_up_fast`, `master_down_fast`, `master_mute`. Chords: `Ctrl`, `Alt`, `Shift`, then `F1`–`F24`, `Space`, `Tab`, `Esc`, `Enter`, `Up`/`↑`, `PgUp`, `Home` or a single character. A bare letter is rejected (letters belong to the notes); two commands on the same key is a startup error that names the key.

## Recording

F7 opens the recording settings: the folder (Tab completes directory names; empty = the default folder) and the format. Home starts and End stops recording; the red `● REC mm:ss` indicator blinks once a second while a take runs. The file `YYYY-MM-DD_HH-MM-SS.flac` or `.ogg` appears in `record.dir` (by default the Music folder/sf2_player). FLAC is lossless 24-bit; Ogg Vorbis uses the `record.vorbis_quality` setting. Encoding runs in a separate thread; the audio callback only copies samples into a ring buffer, and an overflow is shown in the status line as `rec lost N`. Quitting while recording asks for confirmation.

## Indicators

- Settings lines `[F2]`–`[F7]`: the chosen MIDI input, audio output (green `OK` while the stream runs), bank file, preset, effects and recording folder/format. A problem is shown in red in the line it belongs to (no device, disconnected port, open error, missing bank); messages that belong to no line (device fallback, recording saved, config not saved) appear in yellow under the list.
- Meter: peak and RMS of the left and right channels in dBFS, updated 30 times per second; a red `CLIP` stays for 1 s after clipping.
- Status in the bottom border, fixed width so the fields never move: `voices 7/64` — the number of held notes and the engine's polyphony limit (rustysynth does not publish the active voice count, so release tails and layered presets are not counted); `cpu 3%` — audio callback time relative to the block duration; `xrun` — buffer underruns; `kbd:` — text keyboard mode, octave and velocity. Transposition, the omni channel and dropped-event counters appear on the right side of the border when active and fit.
- `--ascii` (or `ui.ascii = true`) replaces block characters with `#`, `-`, `*` for terminals without Unicode; the legacy Windows console and `TERM=linux`/`dumb` are detected automatically.

## Latency

Target: at most 15 ms from key to sound with a 256-frame buffer at 48 kHz. How to measure: connect the sound card output to its input (or record both signals with an external recorder), capture the moment the MIDI note is pressed and the first non-zero output sample at the same time; the difference is the latency. On Windows, WASAPI in shared mode is used (ASIO is not supported); if the latency is high, check that the device accepts a 256-frame buffer (`--list-devices` shows the range) and that there is no extra audio processing in the system. On Linux, use the ALSA `hw:` device directly whenever possible.

## Limitations

- MIDI file playback and recording, SF3 and a custom synthesizer are not part of version 1.0.
- The MIDI monitor and per-channel mixer strips of version 1.0 are not part of this screen; channel volume follows incoming CC 7 only.
- The F5 preset applies to every channel except 10 (percussion); a Program Change from the MIDI keyboard still switches its own channel until the next F5 choice, and the header keeps showing the F5 choice.
- Toggling effects on or off (F6, Alt+R, Alt+C) recreates the synthesizer: sounding notes are cut off. Changing a send level in F6 does not.

## License

MIT or Apache-2.0 at your option (`LICENSE-MIT`, `LICENSE-APACHE`). Dependencies are MIT/Apache-2.0, except the transitive `option-ext` (MPL-2.0) from the `directories` crate.
