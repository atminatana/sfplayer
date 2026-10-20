# PROJECT_MAP — sf2_player

Консольный SoundFont-синтезатор на Rust. ТЗ: `docs/spec/2026-10-20-sf2-player-v1.md`. План: `docs/plans/2026-10-20-sf2-player-v1.md`. Архитектурные решения и их обоснования: `docs/adr/README.md`.

## Структура

| Путь | Назначение | Статус |
|------|-----------|--------|
| `src/lib.rs` | Корень библиотеки: объявляет модули для бинарника и тестов | шаг 1.2 |
| `src/main.rs` | Запуск: CLI → конфиг → `Settings` → локаль → `Session` → TUI; `--list-devices` | шаг 3.1 |
| `src/session.rs` | `Session`: аудиопоток и его смена, кольца, MIDI (подключение/переподключение), hot-plug, банк, пресеты каналов, конфиг, `Notice` | шаг 3.3 |
| `src/cli.rs` | Аргументы командной строки (ТЗ §5), `DeviceSelector` имя/индекс + `find` | шаг 1.1, 1.5 |
| `src/logging.rs` | `tracing` в файл в локальном каталоге данных ОС (N-07) | шаг 1.1 |
| `src/synth/` | Трейт `SynthEngine` (mod.rs), `SoundFontBank` + `PresetInfo` (bank.rs), `RustySynthEngine` (rusty.rs) (ADR 0001) | шаг 1.2 |
| `src/midi/` | `MidiEvent` + `parse_message` (parse.rs), порты midir и `connect_input` (ports.rs), `HotplugWatcher` + `MidiStatus` (hotplug.rs) | шаг 1.3–2.3 |
| `src/audio/` | `AudioShared` атомики (shared.rs), `BlockRenderer` + `Command` ядро коллбэка (render.rs), устройства и `select_output_device` с fallback (device.rs), `open_output_stream` (stream.rs) (ADR 0002); микшер, измеритель — план 4.2, 4.3 | шаг 1.4 |
| `src/config.rs` | `Config` (все секции, ADR 0007), load/save, `Settings::resolve` CLI > конфиг > умолчания, `project_dirs` | шаг 2.1 |
| `src/ui/` | `app.rs` цикл ≤ 30 fps + `Request`/`fulfil`, `screen.rs` `View` + экран, `modals.rs` отрисовка модальных окон, `selector.rs` модальный список, `dialog.rs` диалоги F2/F3/F5/F6, `path_input.rs` поле пути F4, `keys.rs` привязки + справка из них, `i18n.rs` локаль (ADR 0004, 0008, 0009) | шаг 3.5 |
| `src/record/` | Запись FLAC / Ogg Vorbis в отдельном потоке (ADR 0005) | план 5.3 |
| `locales/app.yml` | Словарь rust-i18n v2, en и ru рядом для каждого ключа | шаг 3.1 |
| `assets/sin_wave.sf2` | Минимальный банк для оффлайн-тестов | есть |

## Сделано

- Этап 1, шаг 1.1: пакет переименован в `sf2_player` (ADR 0006), CLI со всеми ключами ТЗ §5 и `--log-level`, логирование в файл.
- Этап 1, шаг 1.2: трейт `SynthEngine`, загрузка SF2 и список пресетов, движок rustysynth; оффлайн-тесты RMS на `assets/sin_wave.sf2`.
- Этап 1, шаг 1.3: `MidiEvent` и парсер байтов со всеми типами F-07.
- Этап 1, шаг 1.4: аудиовыход cpal — перечисление, поток stereo f32 с fallback буфера, lock-free ядро коллбэка.
- Этап 1, шаг 1.5: MIDI-порты midir, подключение, цепочка MIDI → кольцо → синтез → аудио собрана в main. Этап 1 завершён; приёмка «звук с клавиатуры» ждёт MIDI-устройства.
- Этап 2, шаг 2.1: TOML-конфиг со всеми секциями, приоритет CLI > конфиг > умолчания.
- Этап 2, шаг 2.2: выбор аудио/MIDI по имени или индексу, fallback на умолчание с предупреждением.
- Этап 2, шаг 2.3: наблюдатель hot-plug (опрос 2 с), статус MIDI Connected/Disconnected/NoPort. Этап 2 завершён.
- Этап 3, шаг 3.1: TUI-каркас (ratatui), `Session`, словари ru/en, движок подменяется командой без перезапуска потока.
- Этап 3, шаг 3.2: главный экран по макету, «золотой» снэпшот 80×24, фокус Tab.
- Этап 3, шаг 3.3: селектор с фильтром, диалоги MIDI/аудио/пресет/формат записи, смена устройств на лету, F8.
- Этап 3, шаг 3.4: диалог F4 — поле пути с автодополнением, недавние банки, ошибка внутри диалога; bracketed paste.
- Этап 3, шаг 3.5: справка F1 из таблицы привязок, прокрутка. Этап 3 завершён.

## Дальше

- Шаг 4.1: монитор MIDI — кольцо MIDI → UI, буфер 200 записей, прокрутка, Ctrl+L, Ctrl+F, имена нот.
- Остальные шаги — по плану `docs/plans/2026-10-20-sf2-player-v1.md`, раздел 4.

## Проверка

```bash
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```
