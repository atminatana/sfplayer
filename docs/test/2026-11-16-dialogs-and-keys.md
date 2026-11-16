# Отчёт о тестах: управляющие элементы на контролах, подтверждение отмены записи, раскладка, деление файлов (v1.3)

- **id:** `2026-11-16-dialogs-and-keys`
- Запуск: `cargo test`; формат и линт: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.

Разделы добавляются по шагам плана.

## Базовый прогон (2026-11-16)

До первой правки, на `1d39161`: 348 unit + 3 i18n + 4 no_alloc + 2 readme = 357 passed, 0 failed; формат и линт чистые (журнал `target/step13/baseline.log`).

## Шаг 1.1 (2026-11-16) — лицензии

Тесты не добавлялись и не менялись: шаг правит только документы. Прогон после шага — те же 357 passed, 0 failed (`target/step13/gates-1.1.log`).

## Шаг 2.1а (2026-11-16) — сессия без устройств

| Тест | Файл | Что проверяет |
| --- | --- | --- |
| `takes_go_to_the_folder_from_the_config` | `src/session/tests.rs` | папка записей берётся из конфига |
| `without_a_configured_folder_takes_go_to_a_folder_of_the_program` | там же | без настройки папка — собственная папка программы в системном каталоге |
| `a_take_does_not_start_without_an_audio_stream` | там же | без аудиопотока запись не начинается и ошибки нет |
| `stopping_and_cancelling_do_nothing_when_no_take_runs` | там же | стоп и отмена без записи ничего не меняют |
| `an_idle_session_has_no_midi_port` | там же | у сессии без устройств порта нет |
| `connecting_a_port_without_an_audio_stream_changes_nothing` | там же | без кольца в аудио-поток порт не подключается, состояние и конфиг прежние |
| `disconnecting_leaves_no_port` | там же | отключение сбрасывает состояние порта |

Прогон: 364 passed, 0 failed (`target/step13/gates-2.1a.log`). Ограничение: пути с настоящим аудио- или MIDI-устройством тестами не покрываются.

## Шаг 2.1б (2026-11-16) — деление `session.rs`

Тесты не добавлялись и не менялись. Прогон: 364 passed, 0 failed (`target/step13/gates-2.1b.log`). Проверка тождества перенесённого текста — сверкой строк, вне `cargo test` (`target/step13/compare_lines.py`).
