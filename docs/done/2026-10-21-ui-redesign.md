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

## Шаг 2 — диалог записи F7: каталог и формат, мигающий индикатор (2026-10-21)

**Сделано**

- `src/ui/record_dialog.rs` (новый): поле каталога на `PathInput` без списка недавних (Tab дополняет только каталоги), строка формата; ↑↓ — фокус, ←→/Space на строке формата — переключение FLAC ↔ Ogg Vorbis, Enter → `Accepted { dir, format }` (`dir = None` при пустом поле), Esc → `Cancelled`; вставка из терминала всегда идёт в поле.
- `src/ui/dialog.rs`: селектор формата v1 удалён (`DialogKind::RecordFormat`, `Dialog::record_format`, `RECORD_FORMATS`, `Choice::SetRecordFormat`); добавлен `Choice::SetRecording { dir, format }`.
- `src/session/control.rs`: `set_record_format` → `set_recording(dir, format)`.
- `src/ui/app.rs`: `Modal::Record`, `Request::OpenRecordDialog`; Enter в диалоге → `Apply(SetRecording)` → конфиг + `save_config`; `handle_paste` обслуживает и диалог записи.
- `src/ui/modals.rs`: `render_record_dialog` — «Folder:» с курсором (серая подсказка при пустом поле), «Format: (●) FLAC   ( ) Ogg Vorbis» (инверсия при фокусе), описание формата из v1, подсказка.
- `src/ui/screen.rs`: `record_blink` — точка индикатора горит чётные полупериоды по времени записи (период 1 с), в тёмной фазе — пробел той же ширины.
- `locales/app.yml`: `dialog.record_title` → «Recording/Запись», новые `dialog.record_dir|record_dir_default|record_format|record_hint`; `help.action.record_dialog` → «каталог и формат».
- README.md / README.ru.md: раздел «Запись» (каталог в F7, мигающий индикатор), таблицы клавиш перегенерированы.

**COMPLETENESS** — ТЗ §4.3: диалог (код + 3 unit-теста состояния + тест потока в `app` + снэпшот en/ru), сохранение в конфиг (`SetRecording` → `set_recording` + `save_config`), мигание (код + тест фаз и постоянной ширины). Создание каталога при старте записи — существующий `create_dir_all` в `start_recording`.

**CORRECTNESS** — Home внутри диалога двигает курсор поля, а не запускает запись (диалог получает клавиши первым — тест); пустое поле даёт `None`, что сохраняет поведение «каталог музыки ОС»; дополнение по Tab не предлагает файлы; фаза мигания: 0–499 мс горит, 500–999 мс темно, 1000 мс снова горит.

**EVIDENCE** — `cargo fmt --check` ok; `cargo clippy --all-targets -- -D warnings` чисто; `cargo test`: 182 unit + 3 i18n + 2 readme = 187 passed, 0 failed. LOC: record_dialog.rs 193, modals.rs 341, dialog.rs 201, screen.rs 454, **app.rs 508** — порог 500 превышен на 8 строк двумя ветками `Modal::Record`; файл остаётся одним циклом событий + диспетчером запросов; шаг 4 удаляет из него фокус панелей и монитор (≈ 40 строк), после чего порог соблюдён — при невыполнении будет выделен обработчик модальных окон.

**⚠ WARNINGS**

- `app.rs` 508 LOC > 500 (см. EVIDENCE) — закрывается на шаге 4.
- Тест блинка первой версии был написан с неверной фазой (999 мс ожидалась горящей); исправлен тест, не код: по ТЗ период 1 с — 0–499 горит, 500–999 темно.
- Ошибка создания каталога записи пока показывается в строке статуса (notice), как в v1; перенос в строку F7 — шаг 4.

## Шаг 3 — пресет F5 для всех каналов (2026-10-21, ADR 0011)

**Сделано**

- `src/config.rs`: `synth.preset: Option<(u16, u8)>` — выбор F5 как `[bank, program]`; round-trip в тесте.
- `src/session.rs`: поле `preset` вместо массива по каналам; `load_bank` восстанавливает пресет из конфига только для того же файла банка и только если пресет в банке есть, иначе сбрасывает; `install_engine` шлёт глобальный пресет после замены движка.
- `src/session/control.rs`: `select_preset(bank, program)` → `push_global_preset` (CC 0 + Program Change на каналы 0..16 кроме `PERCUSSION_CHANNEL` = 9) + запись в конфиг; `preset_for()`/`preset_info()` без канала.
- `src/ui/app.rs`: F5 открывает селектор с текущим глобальным выбором; Enter → `select_preset` + `save_config` (F-14).
- `src/ui/screen.rs`, `src/ui/modals.rs`: шапка «Preset:» без номера канала, заголовок диалога без канала; `render_dialog` без параметра `channel`.
- `locales/app.yml`: `header.preset`, `dialog.preset_title`, `help.action.preset_dialog` — формулировки «все каналы, кроме 10».
- `docs/adr/0011-global-preset.md` + индекс; README ×2: `preset = [0, 0]` в примере конфига, пункт «Ограничения» о Program Change с устройства; таблицы клавиш перегенерированы.

**COMPLETENESS** — ТЗ §4.1: применение ко всем каналам кроме 10 (код + тест кольца команд), сохранение/восстановление (код + round-trip конфига), строка шапки (снэпшот), диалог (существующие тесты `Dialog::preset`).

**CORRECTNESS** — тест `global_preset_selects_bank_and_program_on_every_channel_but_percussion` читает кольцо команд и проверяет ровно 15 пар CC 0 + PC на каналах 1–9, 11–16 с нужными значениями; восстановление при старте отбрасывает пресет чужого банка (сравнение путей до перезаписи `synth.sf2`).

**EVIDENCE** — `cargo fmt --check` ok; `cargo clippy --all-targets -- -D warnings` чисто; `cargo test`: 183 unit + 3 i18n + 2 readme = 188 passed, 0 failed. LOC: session.rs 466, control.rs 315, config.rs 415, app.rs 507 (см. предупреждение шага 2).

**⚠ WARNINGS**

- Критерий ТЗ §7 п. 3 покрыт на уровне кольца команд (что именно уходит в движок); звуковой тест «нота на канале 16 звучит выбранным пресетом» на `sin_wave.sf2` невозможен — в банке один пресет, Program Change не меняет звук.
- Восстановление пресета при старте и его сброс при смене банка (`load_bank`) не покрыты автотестом — нужна живая `Session`.
- `current_channel` в `Session` остаётся до шага 4 (его ещё используют полосы микшера).
