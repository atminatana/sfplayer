# Отчёт о тестах: редизайн главного экрана (v1.1)

- **id:** `2026-10-21-ui-redesign`
- Запуск: `cargo test`; формат и линт: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.

Разделы добавляются по шагам плана.

## Шаг 1 (2026-10-21)

**Состав**

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `space_toggles_the_highlighted_effect_and_arrows_switch_rows` | `ui/effects_dialog.rs` | Space переключает эффект текущей строки, ↑↓ меняют строку |
| `levels_move_in_steps_of_five_and_clamp_without_reporting_a_change` | `ui/effects_dialog.rs` | ←→ ±5, пределы 0 и 127, на границе — `Pending` |
| `enter_accepts_and_esc_rolls_back_only_after_a_change` | `ui/effects_dialog.rs` | Enter → `Accepted`, Esc → `Cancelled(None)` без изменений / `Cancelled(Some(initial))` после |
| `effects_settings_round_trip_through_the_config_and_clamp_levels` | `session/control.rs` | `from_config` зажимает уровни, `apply_to` пишет четыре поля и не трогает остальные |
| `f6_dialog_applies_changes_live_saves_on_enter_and_rolls_back_on_esc` | `ui/app/tests.rs` | поток модального окна: `Effects(Set)` при изменении, `SaveEffects` по Enter, откат по Esc, горячие клавиши не протекают |
| `home_and_end_drive_recording_and_quit_asks_for_confirmation_while_recording` | `ui/app/tests.rs` | Home → `StartRecord`, End → `StopRecord`, подтверждение выхода при записи как раньше |
| `function_keys_request_dialogs_and_rescan` (обновлён) | `ui/app/tests.rs` | F6 → `OpenEffectsDialog`, F7 → диалог записи |
| `function_keys_and_ctrl_c_map_to_actions` (обновлён) | `ui/keys/tests.rs` | F6/F7/Home/End → новые действия |
| `effects_dialog_overlay_shows_switches_levels_and_the_highlighted_row` | `ui/screen/tests.rs` | снэпшот диалога en/ru, инверсия текущей строки |
| `full_frame_matches_the_layout_mockup`, `english_screen_shows_devices_bank_and_hints` (обновлены) | `ui/screen/tests.rs` | строка подсказок «Home/End rec» |
| `readmes_contain_the_generated_hotkey_tables` | `tests/readme_keys.rs` | обе таблицы README совпадают с новой раскладкой |

**Результат** — 177 unit + 3 i18n + 2 readme = 182 passed, 0 failed; unit-прогон 0.80 s.

**Ограничения** — применение `EffectsAction::Set` к живой сессии (пересоздание движка / CC 91, 93) не покрыто: `Session` требует устройств cpal; путь проверен до границы `Request` и в `EffectsSettings`.

## Шаг 2 (2026-10-21)

**Состав**

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `enter_reports_the_folder_or_none_when_empty_and_esc_cancels` | `ui/record_dialog.rs` | пустое поле → `dir: None`, текст → `Some(path)`, формат из состояния, Esc |
| `arrows_move_the_focus_and_switch_the_format_only_on_the_format_row` | `ui/record_dialog.rs` | ↑↓ фокус; ←→/Space меняют формат только на строке формата; набор текста на строке формата не попадает в поле |
| `the_field_types_pastes_and_completes_directories_only` | `ui/record_dialog.rs` | Tab дополняет каталог, игнорируя файл с тем же префиксом; вставка идёт в поле при любом фокусе |
| `f7_dialog_takes_paste_into_the_folder_and_reports_folder_and_format_on_enter` | `ui/app/tests.rs` | модальный поток: вставка, Home не запускает запись, Enter → `Apply(SetRecording)`, Esc закрывает |
| `function_keys_request_dialogs_and_rescan` (обновлён) | `ui/app/tests.rs` | F7 → `OpenRecordDialog` |
| `record_dialog_overlay_shows_folder_format_radio_and_description` | `ui/screen/tests.rs` | снэпшот en (пустое поле, FLAC) и ru (каталог, Ogg Vorbis, качество) |
| `recording_dot_blinks_with_a_one_second_period_without_changing_the_width` | `ui/screen/tests.rs` | точка горит/гаснет по фазе, строка не сдвигается; `record_blink` на 499/500/1000 мс |
| `record_dialog_lists_both_formats_with_descriptions` (удалён) | `ui/dialog.rs` | селектор формата v1 больше не существует |

**Результат** — 182 unit + 3 i18n + 2 readme = 187 passed, 0 failed; unit-прогон 0.78 s.

**Ограничения** — `Session::set_recording` и создание каталога при старте записи не покрыты автотестом (нужна живая сессия с аудиоустройством); покрыт путь UI → `Request`.

## Шаг 3 (2026-10-21)

**Состав**

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `global_preset_selects_bank_and_program_on_every_channel_but_percussion` | `session/control.rs` | `push_global_preset` кладёт CC 0 = bank и Program Change = program на каналы 1–9 и 11–16, канал 10 не трогает, других команд нет |
| `save_then_load_round_trips` (обновлён) | `config.rs` | `synth.preset = [128, 5]` сохраняется и читается; в TOML — массив из двух чисел |
| `full_frame_matches_the_layout_mockup`, `english_screen_shows_devices_bank_and_hints` (обновлены) | `ui/screen/tests.rs` | шапка «Preset: 000:000 Grand Piano» без номера канала |
| `readmes_contain_the_generated_hotkey_tables` | `tests/readme_keys.rs` | строка F5 в обоих README — «все каналы, кроме 10» |

**Результат** — 183 unit + 3 i18n + 2 readme = 188 passed, 0 failed; unit-прогон 0.80 s.

**Ограничения** — слышимая смена пресета не проверяется (в `sin_wave.sf2` один пресет); восстановление/сброс пресета в `load_bank` требует живой сессии.

## Шаг 4 (2026-10-21)

**Состав** — `src/ui/screen/tests.rs` переписан под новый макет (18 тестов), `ui/app/tests.rs` (17), `ui/keys/tests.rs` (10):

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `full_frame_matches_the_layout_mockup` | `ui/screen/tests.rs` | золотой кадр 80×24 (en) по ТЗ §2: заголовок по центру, `[F1]…[F7]`, Home/End, микшер, статус в нижней рамке |
| `russian_screen_uses_the_russian_dictionary` | `ui/screen/tests.rs` | те же строки на русском, `−∞`, статус «голоса… клав: gate 300мс окт 4 vel 100» |
| `missing_devices_and_bank_are_red_and_audio_ok_is_green` | `ui/screen/tests.rs` | цвета ячеек: `no device`/`not set` красные, `OK` зелёный, `no bank` серый, `(disconnected)` красный |
| `notices_land_in_the_line_they_belong_to_or_in_the_message_line` | `ui/screen/tests.rs` | шесть уведомлений: строка и цвет (F2/F3/F4/F7 красным, строка 9 жёлтым); ошибка аудио заменяет текст устройства |
| `status_fields_keep_their_columns_whatever_the_values` | `ui/screen/tests.rs` | колонки `cpu/xrun/kbd/oct/vel` не сдвигаются при 999/256, 100 %, 99999, `9999+`, release/gate |
| `optional_status_fields_sit_on_the_right_and_never_push_the_fixed_ones` | `ui/screen/tests.rs` | tr/omni/dropped справа при 100 колонках, скрыты при 80, `tr +12` помещается |
| `recording_indicator_blinks_in_the_recording_line_and_quit_asks_for_confirmation` | `ui/screen/tests.rs` | `● REC` в строке F7, фазы мигания, красный цвет, диалог подтверждения, Ogg Vorbis в строке |
| `mixer_shows_meters_clip_mark_and_master_mute` | `ui/screen/tests.rs` | дБFS, CLIP, `MASTER █████░░░░░   50%  mute`, нет `ch01` |
| `every_line_fits_eighty_columns` | `ui/screen/tests.rs` | длинный каталог и все необязательные поля — ровно 80 ячеек |
| `help_overlay_lists_the_bindings_in_both_languages` (обновлён) | `ui/screen/tests.rs` | нет Tab, есть Home/«Start recording», последняя строка по прокрутке |
| `dialog_overlay…`, `sf2_path_dialog…`, `effects_dialog…`, `record_dialog…`, `ascii_mode…` (обновлены) | `ui/screen/tests.rs` | оверлеи поверх нового экрана; ASCII-глифы в микшере и индикаторе |
| `unbound_keys_do_nothing_and_need_no_redraw` (обновлён), `master_keys_work_everywhere`, `text_keyboard_plays_notes_on_channel_one…` | `ui/app/tests.rs` | Tab/стрелки/цифры — ничего; мастер-клавиши без фокуса; ноты всегда на канале 1 |
| `panel_monitor_and_channel_keys_are_gone`, `config_overrides…` (обновлён), `bad_overrides…` (обновлён) | `ui/keys/tests.rs` | удалённые клавиши и имена команд не распознаются; `record` из v1 — неизвестная команда |
| удалены | `ui/monitor.rs`, `midi/monitor.rs`, `audio/mixer.rs`, `ui/app/tests.rs` | 4 теста монитора UI, 3 теста формата монитора, 2 теста каналов микшера, тесты фокуса/монитора/каналов в `app` |

**Результат** — 165 unit + 3 i18n + 2 readme = 170 passed, 0 failed; unit-прогон 0.90 s.

**Ограничения** — живой прогон в терминале не выполнен (см. ⚠ WARNINGS шага 4); цвета и раскладка подтверждены `TestBackend`.

## Шаг 5 (2026-10-21)

**Состав** — новых тестов нет; итог по задаче: 165 unit (ui 82, audio 28, midi 17, synth 12, cli 11, config 8, record 5, session 2 — по `cargo test -- --list`) + 3 i18n + 2 readme = 170.

**Результат** — 170 passed, 0 failed; unit-прогон ≈ 0.9 s.

**Ограничения** — живой прогон TUI и `Session` с устройствами не покрыты (как и в v1.0).
