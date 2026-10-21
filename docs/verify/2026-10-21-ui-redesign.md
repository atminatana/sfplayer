# Верификация: редизайн главного экрана (v1.1)

- **id:** `2026-10-21-ui-redesign`
- **ТЗ:** `docs/spec/2026-10-21-ui-redesign.md`; **план:** `docs/plans/2026-10-21-ui-redesign.md`
- **Дата:** 2026-10-21
- **Метод (BP-09):** для каждого критерия ТЗ §7 открыт код и тест; `cargo test -- --list` → 170 тестов (165 unit + 3 i18n + 2 readme). Статусы: ✓ — подтверждено кодом и автотестом; ◐ — код есть, автотест невозможен; ✗ — не подтверждено.

## Критерии приёмки (ТЗ v1.1 §7)

| # | Критерий | Статус | Подтверждение |
|---|---|---|---|
| 1 | Снэпшот 80×24 совпадает с макетом §2; заголовок по центру, статус в нижней рамке | ✓ | `ui/screen/tests.rs::full_frame_matches_the_layout_mockup` (золотой кадр en), `russian_screen_uses_the_russian_dictionary`; `screen.rs::render` — `title_top(…Center)`, `title_bottom(…Left/Right)` |
| 2 | Ошибка MIDI / аудио / SF2 красным в своей строке, исчезает при исправлении; аудио без ошибки — зелёное OK | ✓ | `missing_devices_and_bank_are_red_and_audio_ok_is_green`, `notices_land_in_the_line_they_belong_to_or_in_the_message_line` (цвета ячеек `TestBackend`); `screen.rs::NoticeTarget`/`View::line_error` — ошибка показывается только пока `Session::notice` её содержит |
| 3 | F5 меняет пресет на всех каналах, кроме 10: оффлайн-тест подтверждает PC на каналах 1 и 16 и его отсутствие на 10 | ✓ | `session/control.rs::global_preset_selects_bank_and_program_on_every_channel_but_percussion` (кольцо команд: CC 0 + PC на 0..16 кроме 9); `push_global_preset`, `PERCUSSION_CHANNEL`; **уточнение:** тест на уровне команд, не звука — в `sin_wave.sf2` один пресет |
| 4 | F6: вкл/уровень реверба и хоруса меняются из диалога и сохраняются в конфиг | ✓ / ◐ | `ui/effects_dialog.rs` (3 теста), `ui/app/tests.rs::f6_dialog_applies_changes_live_saves_on_enter_and_rolls_back_on_esc` (`Effects(Set)` → `SaveEffects`), `control.rs::effects_settings_round_trip…`; ◐ — `apply_effects(Set)` на живой сессии (пересоздание движка/CC 91, 93) и `save_config` требуют устройств |
| 5 | F7: каталог и формат меняются из диалога и сохраняются; Home/End — старт/стоп, файл в каталоге; индикатор мигает | ✓ / ◐ | `ui/record_dialog.rs` (3 теста), `app/tests.rs::f7_dialog_takes_paste…`, `home_and_end_drive_recording…`, `screen/tests.rs::recording_indicator_blinks_in_the_recording_line…`; `Session::set_recording`, `start_recording` (`create_dir_all` + `Recorder::start`, как в v1); ◐ — появление файла на диске и сохранение конфига не воспроизводятся без аудиоустройства |
| 6 | Строка статуса не меняет ширину при `held` 0→999, `xrun` 0→99999, смене режима клавиатуры | ✓ | `status_fields_keep_their_columns_whatever_the_values` (позиции `cpu/xrun/kbd/oct/vel`, `9999+`), `optional_status_fields_sit_on_the_right_and_never_push_the_fixed_ones` |
| 7 | Справка F1 и таблицы README ×2 совпадают с раскладкой (`readme_keys` зелёный) | ✓ | `tests/readme_keys.rs::readmes_contain_the_generated_hotkey_tables` — 30 строк в обоих README; `help_overlay_lists_the_bindings_in_both_languages`; `keys/tests.rs::help_lists_every_bound_action_with_its_chords`, `panel_monitor_and_channel_keys_are_gone` |
| 8 | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` зелёные | ✓ | прогон 2026-10-21 после шага 5: fmt ok, clippy без предупреждений, 170 passed, 0 failed |

## Решения владельца (ТЗ §6)

| # | Решение | Где применено |
|---|---|---|
| ОР-1 | пресет для всех каналов, кроме 10 | ADR 0011; `Session::select_preset`, `config.synth.preset` |
| ОР-2 | убрать монитор и полосы каналов | ADR 0010; удалены `ui/monitor.rs`, `midi/monitor.rs`, `ChannelStrip`, кольцо MIDI→UI; клавиши в `keys.rs` |
| ОР-3 | F6 — диалог | `ui/effects_dialog.rs`, `modals.rs::render_effects_dialog` |

## Расхождения с ТЗ, зафиксированные в ТЗ правкой

- §4.4: токен режима клавиатуры дополняется до 16 знаков **вместе с префиксом** `kbd:`/`клав:` (в черновике — 14 без префикса); разделители полей — один пробел; необязательные поля — справа в нижней рамке и скрываются целиком, если не помещаются (при 80 колонках помещается одно короткое поле).

## Не проверено (нужно ваше участие)

- Живой прогон TUI: вид экрана в реальном терминале (Windows Terminal / Linux), цвета, диалоги F6/F7, запись Home/End с файлом в указанном каталоге, мигание индикатора.
- Восстановление пресета при старте с тем же банком и его сброс при другом банке (`load_bank`) — логика покрыта только чтением кода.
