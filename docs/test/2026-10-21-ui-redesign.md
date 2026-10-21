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
