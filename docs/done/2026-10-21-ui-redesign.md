# Отчёт о выполнении: редизайн главного экрана (v1.1)

- **id:** `2026-10-21-ui-redesign`
- **План:** `docs/plans/2026-10-21-ui-redesign.md`
- **ПОЧЕМУ** решений — в `docs/adr/`, здесь только WHAT.

Разделы добавляются по мере завершения шагов плана.

## Шаг 1 — диалог эффектов F6, F7 → диалог записи, Home/End (2026-10-21)

**Сделано**

- `src/ui/keys.rs`: F6 → `EffectsDialog`, F7 → `RecordDialog`, Home → `RecordStart`, End → `RecordStop`; действия `RecordFormatDialog`, `Record`, `ScrollHome`, `ScrollEnd` удалены вместе с именами `[keys]` (`record_format_dialog`, `record`, `scroll_home`, `scroll_end`); новые имена `effects_dialog`, `record_dialog`, `record_start`, `record_stop`.
- `src/ui/effects_dialog.rs` (новый): состояние диалога F6 — строка (реверб/хорус), Space — вкл/выкл, ←→ — уровень ±5 в 0..=127, Enter — `Accepted`, Esc — `Cancelled` с откатом только если что-то менялось; `Changed` применяется сразу.
- `src/session/control.rs`: `EffectsSettings` (вкл/выкл + уровни) с `from_config`/`apply_to`, `EffectsAction::Set`, `Session::effects()`; `apply_effects` пересоздаёт движок только при смене «есть ли эффекты», иначе шлёт CC 91/93.
- `src/session.rs`: `start_recording` публичный и идемпотентный (повторный Home игнорируется), `toggle_recording` удалён.
- `src/ui/app.rs`: `Modal::Effects`, запросы `OpenEffectsDialog`, `StartRecord`, `StopRecord`, `SaveEffects` (применить + `save_config`); `ToggleRecord` удалён; F7 пока открывает селектор формата из v1 (поле каталога — шаг 2).
- `src/ui/modals.rs`: `render_effects_dialog` — две строки `Reverb   [x]  level  40`, текущая строка инверсией, подсказка.
- `locales/app.yml`: ключи `dialog.effects_*`, `help.action.effects_dialog|record_dialog|record_start|record_stop`; `hint.record` → «Home/End rec»; удалены `help.action.record_format_dialog|record|scroll_home|scroll_end`.
- README.md / README.ru.md: таблицы клавиш перегенерированы (43 строки), имена команд `[keys]`, раздел «Запись» (F7 — настройки, Home/End — старт/стоп), «Ограничения» (смена уровня в F6 движок не пересоздаёт).

**COMPLETENESS** — ТЗ §4.2 (диалог F6): код + 3 unit-теста состояния + тест потока в `app` + снэпшот; §4.3 Home/End: код + тест `home_and_end_drive_recording…`; §3 клавиши F6/F7/Home/End: `keys/tests.rs`; README ×2 синхронно.

**CORRECTNESS** — откат по Esc отдаёт ровно исходные настройки и только после изменения; Enter даёт `SaveEffects` (применение + запись конфига); уровни зажаты в 0..=127 и на границе не порождают лишних команд; `apply_effects(Set)` идёт тем же путём, что Alt+R/Alt+C.

**EVIDENCE** — `cargo fmt --check` ok; `cargo clippy --all-targets -- -D warnings` чисто; `cargo test`: 177 unit + 3 i18n + 2 readme = 182 passed, 0 failed (baseline до изменений — 176 passed). LOC: app.rs 494, keys.rs 476, modals.rs 263, effects_dialog.rs 183, control.rs 270, session.rs 460 — ниже порогов QG-06 (app.rs — один цикл событий + диспетчер запросов, не делить).

**⚠ WARNINGS**

- Диалог F7 на этом шаге — только формат (селектор v1); каталог записи — шаг 2.
- `Session::apply_effects(Set)` не покрыт тестом с живой сессией (нужны устройства cpal); покрыты `EffectsSettings` и путь UI → `Request`.
- Подсказка в строке статуса «Home/End rec» временная — строка подсказок исчезает на шаге 4.
