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

## Шаг 2.2а (2026-11-16) — `fulfil` на сессии без устройств

Файл `src/ui/app/fulfil_tests.rs`, 14 тестов:

| Тест | Что проверяет |
| --- | --- |
| `a_master_key_changes_the_volume_in_the_session_and_in_the_config` | громкость меняется в сессии и в конфиге, файл не пишется |
| `the_preset_dialog_opens_only_when_a_bank_is_loaded` | окно пресета — только при загруженном банке |
| `the_folder_dialog_is_asked_with_the_folder_and_its_history` | запрос окна папки: вид, начальный путь, история |
| `the_bank_dialog_starts_from_the_loaded_bank_and_the_recent_ones` | запрос окна банка без банка и с ним |
| `the_midi_file_dialog_starts_from_the_remembered_file` | запрос окна MIDI-файла |
| `the_effects_dialog_opens_with_the_current_settings` | окно эффектов открывается с текущими значениями |
| `a_chosen_preset_is_applied_and_saved` | выбранный пресет применён и сохранён |
| `a_chosen_midi_port_saves_the_config` | выбор порта сохраняет конфиг |
| `the_record_format_is_set_and_saved` | формат записи задан и сохранён |
| `a_change_of_the_processor_drops_the_held_notes_and_is_saved` | смена обработчика кнопкой и переключением снимает удержанные ноты и сохраняется |
| `timbre_requests_change_the_config_and_wait_for_the_exit_to_be_saved` | волна и огибающая меняют конфиг, файл не пишется |
| `transposition_and_effect_switches_reach_the_session` | транспонирование и переключатель эффекта |
| `saved_effects_are_applied_and_written` | сохранение эффектов из окна |
| `requests_that_need_the_audio_stream_do_nothing_without_it` | запись, транспорт, ноты и Panic без аудиопотока безвредны |

Прогон: 378 passed, 0 failed (`target/step13/gates-2.2a.log`). Ограничение: ветки с настоящими устройствами не покрываются.
