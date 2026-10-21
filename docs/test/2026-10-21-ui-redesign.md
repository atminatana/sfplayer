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
