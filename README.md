# sf2_player

[English](README.md) | [Русский](README.ru.md)

SoundFont synthesizer in Rust in its own window with a grid of character cells: notes from a MIDI keyboard (or from the two bottom rows of a regular keyboard) are played through the chosen audio output with a SoundFont 2 (.sf2) bank or with the built-in synthesizer; a MIDI file is played the same way. The screen follows the signal path — MIDI sources, the processor, the output, recording to a file: it shows the chosen devices, bank, preset, effects, the output level and the master volume; a session can be recorded as sound to FLAC or Ogg Vorbis, or as the notes themselves to a MIDI file. Windows and Linux; the interface is in English or Russian.

## Installation

**Prebuilt binaries** — the Releases tab: archives for Windows x86_64 and Linux x86_64. Unpack and run `sf2_player`.

**Build from source** (Rust stable, edition 2024):

```bash
cargo build --release
./target/release/sf2_player --sf2 path/to/bank.sf2
```

What the machine needs:

| OS | Dependencies |
|---|---|
| Windows 10+ | Rust toolchain with MSVC (a C compiler is needed for Ogg Vorbis; it ships with Build Tools) |
| Linux | `libasound2-dev` (ALSA, for cpal), `gcc` or `clang`, `pkg-config`; for the window — X11 or Wayland and fontconfig with any monospace font; PipeWire and PulseAudio work through the ALSA compatibility layer |

The program opens its own window, so it needs a graphical environment: it does not start in a bare console or over SSH. OpenGL is optional — without it the window is drawn in software.

SF2 banks are not shipped with the program. Free General MIDI banks: GeneralUser GS, FluidR3, TimGM6mb.

## Running

```
sf2_player [--sf2 PATH] [--midi-in NAME|INDEX] [--audio-out NAME|INDEX]
           [--buffer FRAMES] [--sample-rate HZ] [--config PATH]
           [--log-level LEVEL] [--backend KIND] [--list-devices]
```

- `--list-devices` prints MIDI inputs and audio outputs with their indices and exits.
- Devices whose configuration cannot be read (an unsupported ALSA hint, a busy device) are skipped with a warning in the log instead of breaking the whole list.
- Devices are selected by index or by a case-insensitive substring of the name; if a device is not found, the system default is used (the first port for MIDI) and a warning appears in the message line. If a device is found but cannot be opened (busy, unsupported configuration), the program runs without sound, the audio output line shows the cause in red, and another device is chosen with F3.
- The default buffer is 256 frames; if the device does not accept it, the device default is used.
- Command-line values take precedence over the config file.
- `--backend auto|gl|gles|soft` chooses how the window is drawn; by default — the `TERM_BACKEND` environment variable, otherwise automatically: OpenGL 3.3, then OpenGL ES 3.0, then software rendering.

Without `--sf2` the program starts silent: a bank is chosen with F4. You can also do without a bank — switch to the built-in synthesizer with the `[Synth]` button.

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
| F7 | Folder for takes |
| F8 | Rescan MIDI ports |
| F9 | Panic: all notes off + all sound off |
| F10 | Quit |
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
| Tab / Shift+Tab | Next / previous button of the screen |
| Enter | Press the button under the focus (or click it) |
| Z X C V B N M , . / | White keys C–E from the base octave (text keyboard) |
| S D G H J L ; | Black keys C# D# F# G# A# C# D# (text keyboard) |
<!-- keys:end -->

While a dialog is open, input goes to the dialog: ↑↓, PgUp/PgDn, Home/End move through lists and fields there, Enter applies, Esc cancels.

The MIDI input and the audio output are chosen from a dropdown in their own line: the field shows the device in use. The list drops down on a click on the field, on Enter while the field has the focus, or on F2 and F3; ↑↓, PgUp/PgDn, Home/End and the wheel move the highlight, Enter or a click on a row connects the device, Esc and a click elsewhere close the list and change nothing. The devices are listed as they are when the list drops down; the rate and the buffer of an audio device are shown in the line once it is chosen.

Labels in square brackets on the main screen are buttons. Press them with the mouse or from the keyboard: Tab and Shift+Tab move the focus from button to button (the button under the focus is underlined), Enter presses it. The `[−]` and `[+]` buttons in the "Master" line change the volume by 5 %. A click outside an open dialog closes it, like Esc.

F4 opens the bank path dialog: as you type, matching folders and `.sf2` files appear under the field; with an empty field — the recent banks. ↑↓ or a mouse click put a row into the field, Enter opens a folder or loads a bank, Tab moves between the field, the list and the `[OK]` and `[Cancel]` buttons, Esc or a click outside the dialog cancels. Selection and the clipboard work in the field (Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+A).

F7 opens the same dialog for the folder of takes: only folders are offered; with an empty field — the folders chosen before. Enter takes what the field holds; an empty field means the default folder. An existing file cannot be given — the dialog reports the error and stays open.

### Text keyboard

```
  S  D     G  H  J     L  ;        ← black keys (C#4 D#4 F#4 G#4 A#4 C#5 D#5)
 Z  X  C  V  B  N  M  ,  .  /     ← white keys (C4 D4 E4 F4 G4 A4 B4 C5 D5 E5)
```

Notes play on MIDI channel 1 with the F5 preset and sound while the key is held. When the window loses focus, held notes are released.

The screen shows a piano of the same range — from the C of the base octave to the E of the next one; when the octave changes (Ctrl+↑/↓) it moves along with the keyboard. The keys of sounding notes are lit wherever the note comes from — the keyboard, the mouse or the MIDI input. The piano can be played with the mouse: a press starts a note and the release stops it; dragging the pointer over the keys with the button down plays them one after another. Mouse notes use the velocity of the text keyboard.

Notes and hotkeys go by the position of the key, not by its character: the input language (Russian, German) does not get in their way. In the config and in the help, keys are named by the characters of the US layout. Text in dialogs — the list filter, a path — is typed in the current layout.

## Processor: Sound Font or Synth

Notes are turned into sound by one of two processors, chosen with the `[Synth]` `[Sound Font]` buttons in the heading of the "Processor" section. The switch happens on the fly; the notes that were sounding end. The choice is remembered: the next time the program plays with the same processor.

**Sound Font** plays an `.sf2` bank — with all its instruments, the F5 preset and percussion on channel 10.

**Synth** is the built-in synthesizer; it needs no bank. One timbre serves all 16 channels (channel 10 plays the same notes, not percussion) and is set on the screen:

- the waveform — `[Sine]`, `[Saw]` or `[Triangle]`;
- the ADSR envelope — the ready-made sets `[Piano]`, `[Harpsichord]`, `[Organ]`, `[Flute]`, or your own values: the `[−]` and `[+]` buttons change Attack, Decay and Release in steps of 0.005 s within 0.001–2 s, and Sustain in steps of 5 % from 0 to 100 %. After a manual change the set is marked "Custom".

New values are taken by the next note; the sounding ones finish with the old. Polyphony is 64 voices, an extra note takes the voice of the oldest. Velocity, channel volume (CC 7), the pedal (CC 64) and a Pitch Bend of ±2 semitones are understood; a program change selects nothing. Reverb and chorus work as with a Sound Font and are controlled by the same F6, Alt+R and Alt+C.

## MIDI file

The third source of notes is a Standard MIDI File (`.mid`, `.midi`, formats 0 and 1). A file is chosen with the `[MIDI file]` button: the path dialog is the one used for a bank — as you type it offers folders and MIDI files; with an empty field, the recent files. A file that cannot be read or is not a MIDI file is not accepted, and the dialog says why.

The `[Start]` and `[Pause]` buttons start and pause playback, `[Stop]` returns the file to its beginning; at its end the file stops by itself. The "position" line shows a bar, the time and the state. There is no seeking.

The file plays together with the keyboard and the MIDI input; transposition and omni apply to it as they do to the MIDI input. Program changes in the file switch the presets of their channels. When the file is stopped or has played to its end, the program ends its notes, resets the controllers, puts back the volume and pan of the channels and applies the F5 preset again — the file leaves nothing behind. The chosen file is remembered and chosen again at the next start, but does not play by itself.

## Configuration

A TOML file is created on the first exit and stores the selected devices, the processor and the timbre of the built-in synthesizer, the bank and the last 10 banks, master volume, effects, the recording folder, the last 10 folders and the recording format:

| OS | Path |
|---|---|
| Windows | `%APPDATA%\sf2_player\config\config.toml` |
| Linux | `~/.config/sf2_player/config.toml` |

The log is written to the local data directory (`%LOCALAPPDATA%\sf2_player\data\sf2_player.log`, `~/.local/share/sf2_player/`); the level is set by `--log-level`. Each start removes the previous run's log, so the file always covers a single session.

All fields are optional; unknown keys are ignored.

```toml
[midi]
input = "KeyStep"          # name substring or index
omni_channel = 1           # route all incoming notes to channel 1 (1–16); remove to disable

[source]
midi_file = "D:/midi/prelude.mid"        # the chosen MIDI file
recent_midi_files = ["D:/midi/prelude.mid"]  # recent ones, up to 10; kept by the program

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
engine = "sf2"             # "sf2" | "builtin" — the built-in synthesizer

[synth.builtin]
wave = "sine"              # "sine" | "saw" | "triangle"
adsr_preset = "piano"      # "piano" | "harpsichord" | "organ" | "flute" | "custom"
attack = 0.003             # seconds, 0.001–2
decay = 0.5
sustain = 0.35             # share of the peak, 0–1
release = 0.35

[mixer]
master = 80                # 0–100 %

[keyboard]
white = "zxcvbnm,./"       # white keys from C
black = "sdghjl;"          # black keys C# D# F# G# A# C# D#
base_octave = 4
velocity = 100
octave_up = "Alt+Up"       # extra chords in addition to Ctrl+arrows
velocity_down = "Alt+Down"

[keys]                     # command = chord; several — comma-separated
quit = "F10, Ctrl+Q"
panic = "F9"
rescan = "Alt+F3"

[ui]
lang = "en"                # "ru" | "en"; remove — system language
font = "Consolas"          # monospace font family; remove — the system one
font_size = 14             # font size in pixels

[record]
dir = "D:/Music/sf2"       # remove — the Music folder/sf2_player
format = "flac"            # "flac" | "ogg_vorbis" | "midi"
vorbis_quality = 0.5       # 0.0–1.0
recent_dirs = ["D:/Music/sf2"]  # folders chosen before, up to 10; kept by the program
```

Command names for `[keys]`: `help`, `quit`, `midi_dialog`, `audio_dialog`, `sf2_dialog`, `preset_dialog`, `effects_dialog`, `record_dialog`, `rescan`, `record_start`, `record_stop`, `panic`, `engine_toggle`, `midi_file_dialog`, `file_play_pause`, `file_stop`, `octave_up`, `octave_down`, `velocity_up`, `velocity_down`, `notes_off`, `transpose_up`, `transpose_down`, `toggle_reverb`, `toggle_chorus`, `master_up`, `master_down`, `master_up_fast`, `master_down_fast`, `master_mute`. Chords: `Ctrl`, `Alt`, `Shift`, then `F1`–`F24`, `Space`, `Tab`, `Esc`, `Enter`, `Up`/`↑`, `PgUp`, `Home` or a single character. A bare letter is rejected (letters belong to the notes); two commands on the same key is a startup error that names the key. Tab, Shift+Tab and Enter on the main screen are not commands and have no `[keys]` names; if a command is bound to one of these keys, the key runs that command and does not act on the buttons. Recording, the MIDI file and the choice of the processor are driven by the buttons on the screen and have no default key: `record_start`, `record_stop`, `midi_file_dialog`, `file_play_pause`, `file_stop` and `engine_toggle` get a key only if you assign one here — for example `file_play_pause = "Space"`. Quit is on F10 alone; Ctrl+C can be added with `quit = "F10, Ctrl+C"`.

## Recording

The folder of takes is chosen with F7 or the `[Folder for files]` button, the format with the `[FLAC]`, `[OGG]` and `[MIDI]` buttons in the "Format" line; the format cannot be changed while a take runs. The `[Start]` button starts and `[Stop]` stops recording. The `[Cancel]` button — red while a take runs, without a hotkey — asks first: a window with the length and the size of the take and the `[Delete]` and `[No]` buttons, the focus on `[No]`; the take goes on while the question is open. `[Delete]` stops the take and deletes its file. While a take runs, the red indicator `● REC mm:ss · size · file name` is shown, its dot blinking once a second; after a take is saved, the line shows a green `saved: path · size` (a path too long for the line loses its beginning). The file `YYYY-MM-DD_HH-MM-SS.flac`, `.ogg` or `.mid` appears in `record.dir` (by default the Music folder/sf2_player). FLAC is lossless 24-bit; Ogg Vorbis uses the `record.vorbis_quality` setting. Encoding runs in a separate thread; the audio callback only copies samples into a ring buffer, and an overflow is shown in the status line as `rec lost N`. With the MIDI format no sound is recorded: the file (Standard MIDI File, format 0) gets the events the synthesizer was fed — the notes of the MIDI input, of the window and of a MIDI file being played, after transposition and omni, and what the program itself sends (Panic, a preset change). Notes still sounding when the take stops are ended in the file. Such a file can be chosen as the MIDI file source and played back. Quitting while recording asks for confirmation: a window with the `[Quit]` and `[No]` buttons, the focus on `[No]`; pressing F10 again quits at once.

## Indicators

- The screen is divided into the sections "MIDI sources", "Processor", "Output" and "Audio file". The lines show the chosen MIDI input (green "open"), the bank file and the audio output (green `OK` while everything works), the preset, effects and the recording folder and format. A problem is shown in red in the line it belongs to (no device, disconnected port, open error, missing bank); messages that belong to no line (device fallback, recording saved or cancelled, config not saved, font replaced) appear in yellow under the recording line. Labels in square brackets are buttons: the chosen option is highlighted, unavailable ones are dimmed, the one whose action cannot be undone is red, the button under the keyboard focus is underlined, the one held down by the mouse is light. A field with a triangle at its right end is a device dropdown; on an audio error the line shows the error text in place of the rate and the buffer.
- Meter: the bar shows the RMS of the left and right channels with colour zones (green, amber from 78 %, red from 92 %), the number is the peak in dBFS; updated 30 times per second, a red `CLIP` stays for 1 s after clipping.
- The bottom line, with counters of fixed width so the fields never move: `voices 7/64` — the number of held notes and the engine's polyphony limit (rustysynth does not publish the active voice count, so release tails and layered presets are not counted); `cpu 3%` — audio callback time relative to the block duration; `xrun` — buffer underruns. Transposition, the omni channel and dropped-event counters follow when active; on the right — the chosen processor and bank. The octave and velocity of the text keyboard are shown in the "Keyboard" line.
- Note lines: "Keyboard" names the notes sounding from the text keyboard and the mouse, "→ notes" those from the MIDI input (`C4 E4 G4`, lowest first, transposition applied; what does not fit is replaced by `…`). Notes outside the range of the piano are visible only here.
- The window can be resized; the grid needs at least 96×31 cells, otherwise a request to enlarge the window is shown instead of the screen. The font and its size are set in the `[ui]` section; if the font is not found, the system monospace font is used.

## Latency

Target: at most 15 ms from key to sound with a 256-frame buffer at 48 kHz. How to measure: connect the sound card output to its input (or record both signals with an external recorder), capture the moment the MIDI note is pressed and the first non-zero output sample at the same time; the difference is the latency. On Windows, WASAPI in shared mode is used (ASIO is not supported); if the latency is high, check that the device accepts a 256-frame buffer (`--list-devices` shows the range) and that there is no extra audio processing in the system. On Linux, use the ALSA `hw:` device directly whenever possible.

## Limitations

- macOS is not supported: the gterm window library runs on Windows and Linux.
- In the selection windows (MIDI input, audio output, preset), the effects window and the help the mouse does not act — choices are made with keys; a click outside the window closes it. The mouse works on the buttons of the main screen and in the path dialogs (F4, F7).
- A MIDI take holds what was played, not the state of the synthesizer when it began: the preset and the effect levels chosen before the take are not in the file, and it plays with the preset chosen at playback. Time in the file is as fine as the audio buffer (about 5 ms with 256 frames at 48 kHz).
- SF3 banks are not supported.
- MIDI files of format 2 and files timed in SMPTE frames are not played; there is no seeking in a file.
- The MIDI monitor and per-channel mixer strips of version 1.0 are not part of this screen; channel volume follows incoming CC 7 only.
- The F5 preset applies to every channel except 10 (percussion); a Program Change from the MIDI keyboard still switches its own channel until the next F5 choice, and the "Preset" line keeps showing the F5 choice.
- With the Sound Font processor, toggling effects on or off (F6, Alt+R, Alt+C) recreates the synthesizer: sounding notes are cut off. Changing a send level in F6 does not; with the built-in synthesizer notes are cut off in neither case.

## License

MIT or Apache-2.0 at your option (`LICENSE-MIT`, `LICENSE-APACHE`). Dependencies are under permissive licences (MIT, Apache-2.0, BSD, ISC, Zlib, BSL-1.0, Unicode-3.0); the one exception, the transitive `option-ext` (MPL-2.0) from the `directories` crate, is used unmodified. The window is built on the [gterm](https://github.com/atminatana/gterm) library. The reverb and the chorus of the built-in synthesizer are ported from [RustySynth](https://github.com/sinshu/rustysynth) (MIT, © 2022 Nobuaki Tanaka); the text of its license is in the `LICENSE-RUSTYSYNTH` file.
