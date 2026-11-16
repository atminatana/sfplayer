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

## Шаг 2.2б (2026-11-16) — деление `app.rs`

Тесты `fulfil` переехали в `src/ui/fulfil/tests.rs` без правок утверждений (изменились только импорты). Прогон: 378 passed, 0 failed (`target/step13/gates-2.2b.log`).

## Шаг 2.3 (2026-11-16) — деление `main_screen.rs`

Тесты не добавлялись; в `src/ui/main_screen/tests.rs` изменились только строки импорта (имена, раньше приходившие через импорты родительского модуля, и два помощника из `output.rs`). Прогон: 378 passed, 0 failed (`target/step13/gates-2.3.log`).

## Шаг 3.1 (2026-11-16) — раскладка по умолчанию

Новые тесты:

| Тест | Файл | Что проверяет |
| --- | --- | --- |
| `the_keys_removed_in_v1_3_are_free_by_default` | `src/ui/keys/tests.rs` | семь убранных клавиш по умолчанию не привязаны ни к чему |
| `a_command_without_a_default_key_takes_one_from_the_config` | там же | шесть команд без клавиши получают её из `[keys]`; выход можно назначить на F10 и Ctrl+C |
| `an_empty_keys_section_gives_the_defaults` | там же | пустой `[keys]` даёт таблицу умолчаний |
| `commands_without_a_default_key_run_from_a_configured_one` | `src/ui/app/tests.rs` | назначенная клавиша даёт запрос своей команды |

Изменённые тесты (ожидания приведены к F-28): `function_keys_map_to_actions`, `help_lists_every_bound_action_with_its_chords` (`keys/tests.rs`); `the_quit_key_sets_the_flag_even_inside_a_dialog`, `f9_requests_panic_and_the_midi_file_has_no_keys_of_its_own`, `quit_asks_for_confirmation_while_recording` и набор команд F-клавиш (`app/tests.rs`); `hotkeys_work_in_a_russian_layout`, `in_a_text_field_only_non_character_quit_keys_quit` (`keymap/tests.rs`); `the_help_lists_the_bindings_and_scrolls` (`dialog_screen/tests.rs`).

Прогон: 382 passed, 0 failed (`target/step13/gates-3.1.log`). `readme_keys` подтверждает, что таблицы обоих README совпадают с раскладкой.

## Шаг 5.1 (2026-11-16) — окно подтверждения, выход во время записи

Новые тесты окна (`src/ui/confirm_dialog/tests.rs`, события — через `route_key` / `route_mouse`, картинка — из `modal_buffer`):

| Тест | Что проверяет |
| --- | --- |
| `the_window_shows_the_question_in_a_red_frame_with_the_focus_on_the_refusal` | размер и место окна, заголовок, вопрос, кнопки, красная рамка, подчёркнутая кнопка отказа; русский и английский тексты |
| `enter_or_space_right_after_opening_refuses` | Enter и пробел сразу после открытия — отказ |
| `tab_moves_the_focus_to_the_action_and_enter_confirms` | Tab и Shift+Tab переводят фокус на действие, Enter подтверждает; Tab дважды возвращает на отказ |
| `esc_and_a_click_outside_the_window_refuse` | Esc и клик вне окна — отказ |
| `a_click_presses_the_button_under_it` | клик по каждой кнопке; клик по тексту вопроса ничего не нажимает |
| `keys_that_are_not_for_the_buttons_mean_nothing` | буква, стрелки, F9, Home ничего не делают и фокус не сдвигают |

Тесты `App` (`src/ui/app/tests.rs`): `quit_asks_for_confirmation_while_recording` переписан под вопрос и ответ (вопрос при записи, клавиши при открытом вопросе не играют и не вызывают команд, отказ, подтверждение, «спрошено дважды — выход», без записи — без вопроса); новый `the_quit_question_takes_the_place_of_an_open_dialog`. Из `dialog_screen/tests.rs` удалён снимок прежнего подтверждения.

Прогон: 388 passed, 0 failed (`target/step13/gates-5.1.log`). Вручную, по снимкам живого окна: показ окна, отказ по Enter, фокус по Tab, выход по Enter на `[Выйти]`, сохранённый файл записи. Не проверено вживую: мышь в окне.
