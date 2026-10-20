# PROJECT_MAP — sf2_player

Консольный SoundFont-синтезатор на Rust. ТЗ: `docs/spec/2026-10-20-sf2-player-v1.md`. План: `docs/plans/2026-10-20-sf2-player-v1.md`. Архитектурные решения и их обоснования: `docs/adr/README.md`.

## Структура

| Путь | Назначение | Статус |
|------|-----------|--------|
| `src/lib.rs` | Корень библиотеки: объявляет модули для бинарника и тестов | шаг 1.2 |
| `src/main.rs` | Разбор CLI, логирование, загрузка банка по `--sf2`; далее — запуск потоков и восстановление терминала | шаг 1.1–1.2 |
| `src/cli.rs` | Аргументы командной строки (ТЗ §5), `DeviceSelector` имя/индекс | шаг 1.1 |
| `src/logging.rs` | `tracing` в файл в локальном каталоге данных ОС (N-07) | шаг 1.1 |
| `src/synth/` | Трейт `SynthEngine` (mod.rs), `SoundFontBank` + `PresetInfo` (bank.rs), `RustySynthEngine` (rusty.rs) (ADR 0001) | шаг 1.2 |
| `src/midi/` | Парсер `MidiEvent`, порты midir, hot-plug | план 1.3, 1.5, 2.3 |
| `src/audio/` | Устройства cpal, поток, коллбэк, микшер, измеритель (ADR 0002) | план 1.4, 4.2, 4.3 |
| `src/config.rs` | TOML-конфиг, каталоги ОС (ADR 0007) | план 2.1 |
| `src/ui/` | ratatui: состояние, виджеты, диалоги, клавиши (ADR 0004, 0008, 0009) | план этап 3–4 |
| `src/record/` | Запись FLAC / Ogg Vorbis в отдельном потоке (ADR 0005) | план 5.3 |
| `locales/` | Словари ru/en для rust-i18n | план 3.1 |
| `assets/sin_wave.sf2` | Минимальный банк для оффлайн-тестов | есть |

## Сделано

- Этап 1, шаг 1.1: пакет переименован в `sf2_player` (ADR 0006), CLI со всеми ключами ТЗ §5 и `--log-level`, логирование в файл.
- Этап 1, шаг 1.2: трейт `SynthEngine`, загрузка SF2 и список пресетов, движок rustysynth; оффлайн-тесты RMS на `assets/sin_wave.sf2`.

## Дальше

- Шаг 1.3: `midi/parse.rs` — `MidiEvent` и парсер байтов с тестами на все типы F-07.
- Остальные шаги — по плану `docs/plans/2026-10-20-sf2-player-v1.md`, раздел 4.

## Проверка

```bash
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```
