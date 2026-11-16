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

## Этап 4 (2026-11-16) — тесты в репозитории gterm

Состав и результаты — `docs/test/2026-11-16-double-click-slider.md` в gterm: +5 тестов счётчика нажатий, +11 в `tests/double_click.rs`, +1 в `tests/input.rs`, +10 в `tests/slider.rs`; 313 → 340 тестов, 0 падений. В sfplayer тесты не добавлялись: в восьми местах, где сообщение мыши собирается вручную, добавлено поле `click_count`. Прогон sfplayer на новой ревизии gterm: 388 passed, 0 failed (`target/step13/gates-4.4.log`).

## Шаг 5.2 (2026-11-16) — подтверждение отмены записи

| Тест | Файл | Что проверяет |
| --- | --- | --- |
| `the_cancel_button_asks_before_a_take_is_thrown_away` | `src/ui/app/tests.rs` | без записи кнопка ничего не спрашивает; при записи открывает вопрос и запрос отмены не отдаёт; отказ — ничего; подтверждение — запрос отмены |
| `a_take_that_ends_by_itself_closes_the_question_about_deleting_it` | там же | вопрос о записи закрывается с её окончанием и просит перерисовку; вопрос о выходе остаётся |
| `the_quit_command_replaces_the_question_about_a_take` | там же | команда выхода при открытом вопросе о записи задаёт вопрос о выходе |
| `the_question_about_a_take_names_its_length_and_size` | `src/ui/confirm_dialog/tests.rs` | заголовок, вопрос с длительностью и размером, кнопки `[Удалить]` `[Нет]`, фокус, красная рамка; английский текст; ответы |
| `a_take_is_worded_by_its_length_and_its_size` | `src/ui/view.rs` | `мм:сс`, размер в КБ и МБ на двух языках, сводка «время · размер» |

Изменён `every_available_button_gives_the_command_of_its_hotkey`: кнопка `[Отмена]` убрана из списка кнопок, сразу дающих запрос.

Прогон: 393 passed, 0 failed (`target/step13/gates-5.2.log`). Вручную, по снимкам живого окна, мышью: вопрос по клику, отказ, удаление, сообщение, исчезновение файла.

## Шаг 6.1 (2026-11-16) — выпадающие списки устройств

`src/ui/controls/tests.rs` — 23 теста: 10 прежних тестов кнопок, переведённых на новый тип (ожидания прежние; первой кнопкой экрана стала `[MIDI-файл]`), и 13 новых или дополненных:

| Тест | Что проверяет |
| --- | --- |
| `a_dropdown_field_shows_the_device_in_use_or_says_in_red_that_there_is_none` | текст поля, значок в последней ячейке, красный цвет без устройства |
| `the_ring_passes_through_the_dropdowns_in_the_order_of_the_screen` | место списков в кольце Tab, подчёркивание поля под фокусом |
| `a_click_on_the_field_or_enter_under_the_focus_drops_the_list_down` | раскрытие кликом и Enter, сообщение окну, значок, место и размер слоя, выделение действующего устройства |
| `the_key_of_a_dropdown_opens_its_list_and_takes_the_focus` | раскрытие по клавише списка; открыт только один список |
| `the_arrows_walk_an_open_list_and_enter_chooses_another_device` | ↓ ↑ Home End и выбор по Enter |
| `choosing_the_device_in_use_closes_the_list_and_asks_for_nothing` | выбор действующего устройства ничего не просит |
| `esc_and_a_click_outside_close_the_list_and_the_next_click_is_an_ordinary_one` | закрытие двумя способами; после закрытия клик нажимает кнопку, а не раскрывает список |
| `a_click_on_a_row_of_the_list_chooses_it` | выбор строки кликом |
| `the_wheel_walks_an_open_list` | колесо двигает выделение |
| `while_a_list_is_open_the_screen_under_it_gets_nothing` | буквы, пробел, F5 и ← не доходят до экрана; клик по кнопке под списком закрывает список и ничего не нажимает |
| `a_dropdown_that_left_the_screen_closes_its_list` | список закрывается, если поле ушло с экрана |
| `a_device_that_is_not_in_the_list_is_still_named_by_the_field` | поле называет устройство, которого ещё нет в списке |
| `before_the_first_frame_and_without_placed_controls_nothing_reacts` (дополнен) | список, которого нет на экране, не раскрывается |

`src/ui/devices.rs` — 3 теста: строки и запросы для MIDI-портов и аудиоустройств, пустой список. `src/ui/main_screen/tests.rs`: снимки двух строк на русском и английском, обход кольца мимо списков, фокус первого кадра на поле MIDI-входа. `src/ui/app/tests.rs`: F2 и F3 просят раскрыть список. `src/ui/fulfil/tests.rs`: запрос окна пресета под новым именем.

Прогон: 408 passed, 0 failed (`target/step13/gates-6.1.log`). Вручную, по снимкам: вид полей, раскрытие по F3 и кликом, закрытие по Esc, клик после закрытия. Не проверено вживую: выбор другого устройства, F2 при наличии MIDI-портов, колесо.

## Шаг 7.1 (2026-11-16) — окно пресета

`src/ui/preset_dialog/tests.rs` — 11 тестов (события через `route_key` / `route_mouse`, картинка из `modal_buffer`):

| Тест | Что проверяет |
| --- | --- |
| `the_window_shows_the_filter_the_presets_and_the_buttons` | размер и место окна, заголовок, поле, строки, отметка действующего пресета, выделение, подсказка, кнопки; короткий список без полосы прокрутки; английский текст |
| `the_list_starts_on_the_preset_in_use_or_on_the_first_one` | начальное выделение: действующий пресет, первый при его отсутствии |
| `typing_narrows_the_list_whatever_the_case` | отбор по названию и по номеру без учёта регистра; Backspace расширяет список |
| `with_no_match_the_window_says_so_and_nothing_can_be_chosen` | «нет совпадений»; Enter, `[OK]` и ↓ ничего не делают; пустой банк — «выбирать нечего» |
| `the_arrows_walk_the_list_from_the_field_and_enter_chooses` | ↑ ↓ PgUp PgDn из поля с упором в края, выбор по Enter |
| `enter_and_ok_choose_the_highlighted_preset` | Enter и `[OK]`; после отбора выбирается пресет, а не место в суженном списке |
| `a_click_highlights_a_row_and_a_double_click_chooses_it` | клик выделяет, двойной клик применяет; двойной клик мимо списка — ничего |
| `esc_cancel_and_a_click_outside_close_the_window_without_choosing` | три способа закрыть без выбора |
| `tab_goes_from_the_field_to_the_list_and_on_to_the_buttons` | кольцо Tab; клавиши списка; Enter в списке и на кнопках |
| `the_wheel_scrolls_a_long_list_and_leaves_the_highlight_where_it_is` | длинный список с полосой прокрутки; колесо листает, выделение на месте |
| `lines_are_matched_by_a_part_of_them_whatever_the_case` | чистая функция отбора |

`src/ui/fulfil/tests.rs`: `the_preset_window_is_asked_for_only_when_a_bank_is_loaded` — без банка окно не запрашивается; с банком запрос несёт пресеты и действующий пресет.

Прогон: 419 passed, 0 failed (`target/step13/gates-7.1.log`). Вручную, по снимку: вид окна по F5. Не проверено вживую: двойной клик, колесо, набор в фильтре.

## Шаг 8.1 (2026-11-16) — окно эффектов

`src/ui/effects_dialog/tests.rs` — 8 тестов (события через `route_key` / `route_mouse`, картинка из `modal_buffer`):

| Тест | Что проверяет |
| --- | --- |
| `the_window_shows_a_switch_a_slider_and_a_level_for_each_effect` | размер и место окна, заголовок, флажки по настройкам, числа уровней у правой рамки, ползунки, кнопки, фокус на первом флажке; английский текст |
| `a_switch_is_turned_by_space_enter_or_a_click_and_the_change_is_reported_at_once` | пробел, Enter и клик переключают флажок; каждое переключение сразу даёт `Changed` с новыми настройками |
| `the_arrows_move_the_level_under_the_focus_by_five_and_home_and_end_go_to_the_limits` | ← → на 5, Home и End — 0 и 127, на границе событий нет, число следует за ползунком; ползунок хоруса меняет только хорус |
| `a_click_on_the_track_sets_the_level_and_a_drag_leads_it` | клик в правый край — 127, в левый — 0, в середину — около половины; протяжка ведёт уровень и за пределами дорожки; после отпускания указатель ничего не ведёт |
| `the_wheel_moves_the_level_under_the_pointer_by_five` | колесо над ползунком: вверх +5, два щелчка вниз −10 |
| `ok_keeps_the_changes` | `[OK]` мышью и с клавиатуры (Tab до кнопки, Enter) даёт `Accepted` с текущими настройками |
| `leaving_without_ok_goes_back_to_what_the_window_opened_with` | Esc, `[Отмена]`, клик вне окна и уход по команде выхода (`cancel`): без изменений — `Cancelled(None)`, после изменения — `Cancelled(исходные)`; изменил и вернул — не изменение |
| `a_level_above_the_range_is_shown_at_its_limit` | уровень из конфига выше 127 показывается как 127 |

`src/ui/window/tests.rs`: `what_the_effects_window_decides_is_applied_at_once_saved_on_ok_and_taken_back_otherwise` — что окно программы делает с решением окна эффектов: изменение — запрос `Effects(Set)` без закрытия, `[OK]` — `SaveEffects` и закрытие, отмена — закрытие и возврат исходных настроек, если было что возвращать.

`src/ui/fulfil/tests.rs`: `the_effects_window_is_asked_for_with_the_current_settings` (прежний `the_effects_dialog_opens_with_the_current_settings`) — запрос окна несёт настройки из конфига.

Удалены вместе с прежним автоматом: три теста в `effects_dialog.rs`, `f6_dialog_applies_changes_live_saves_on_enter_and_rolls_back_on_esc` и блок эффектов в `dismissing_a_dialog_does_what_esc_does_in_it` (`app/tests.rs`), `the_effects_dialog_shows_switches_levels_and_the_current_row` и строки эффектов в двух общих тестах (`dialog_screen/tests.rs`).

Прогон: 423 passed, 0 failed (`target/step13/gates-8.1.log`). Мутации: 11 из 11 пойманы (`target/step13/mutation-8.1.log`). Вручную, по снимкам: вид окна по F6 и начальное состояние флажков. Не проверено вживую: переключение флажка, ползунок клавишами и мышью, `[OK]`, отмена с откатом.
