# План реализации: опциональный бэкенд ASIO (v1.2)

- **id:** `2026-10-22-asio-backend`
- **ТЗ:** `docs/spec/2026-10-22-asio-backend.md` (ОР-1…ОР-3 приняты владельцем 22.10.2026)
- **Статус плана:** в работе
- **Дата:** 2026-10-22
- **Ветка:** `feat/asio-backend` от `develop`

## 1. Рамки

**Входит:** всё из ТЗ §2 «Входит»: feature `asio`, единый список устройств с пометкой `[ASIO]`, поток в формате драйвера, шаг CI, ADR 0012, README ×2, PROJECT_MAP, AGENTS.md, отчёты TL-01.

**Не входит:** поле/флаг бэкенда, панель ASIO, ASIO-вход, релизный артефакт с ASIO, изменения для сборки без feature сверх перечисленных.

## 2. Открытые решения (AR-04)

Все три приняты владельцем 22.10.2026 (ТЗ §7). ADR 0012 пишется на шаге 1, где решение применяется в коде.

**Принятые допущения** (локальные, обратимые — BP-02.3):

- A-1. Стандартная авторизация владельца на коммит после каждого шага (память `autonomous-commit-per-step`, применялась и к плану v1.1) действует и для этого плана; коммиты — в ветку `feat/asio-backend`, без push.
- A-2. Пометка `[ASIO]` — литерал в коде (имя протокола), не ключ словаря (ТЗ Д-1).
- A-3. Загрузка ASIO SDK при первой сборке с feature идёт штатным путём `asio-sys` (во временный каталог); `CPAL_ASIO_DIR` не задаётся. Если сеть или сайт Steinberg недоступны — шаг 2 останавливается с `[data needed]`.
- A-4. Шаг CI проверяется только статически (YAML); первый прогон — после пуша владельцем.

## 3. Шаги

Каждый шаг: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` зелёные (шаг 2 — дополнительно с `--features asio`) → разделы в `docs/done/` и `docs/test/` → README ×2 при видимых изменениях → один коммит.

| Шаг | Содержание | Проверка |
| --- | --- | --- |
| 1 | **Ядро без feature (ADR 0012).** `Cargo.toml`: feature `asio = ["cpal/asio"]`. `audio/device.rs`: `Backend`, `OutputDevice { device, backend, name }`, одно перечисление `enumerate_outputs()` = хост по умолчанию + (при `cfg(all(windows, feature = "asio"))`) хост ASIO с пометкой; `list_output_devices`, `select_output_device`, `output_device_by_index` поверх него; `AudioDeviceInfo.backend`. `audio/render.rs`: `fill`/`interleave` обобщены по `T: cpal::Sample + FromSample<f32>`. `audio/stream.rs`: `open_output_stream(&OutputDevice, …)` выбирает тип потока по `sample_format()` устройства (F32/I16/I32/I24/F64), лог с форматом и бэкендом. `session.rs`: `OutputDevice` вместо `cpal::Device`. ADR 0012 + индекс; ADR 0003 → Superseded | unit: `fill::<i16/i32/I24/f64>` крайние значения и тишина (К-6); объединение списков без cpal — индексы, пометка, `*` (К-7); существующие тесты render/dialog/session зелёные без feature (К-1) |
| 2 | **Сборка с feature и живая проверка.** `cargo clippy --all-targets --features asio -- -D warnings`, `cargo test --features asio` (К-2); `--list-devices` обеих сборок (К-3); запуск с `--audio-out "Realtek ASIO"`, нота, лог формата/буфера (К-4); F3 туда и обратно, `audio.output` в конфиге (К-5). Правки по результатам. `ci.yml`: шаг с `continue-on-error` на `windows-latest` (A-11) | живые прогоны с выдержками лога в `docs/test/`; YAML читается глазами |
| 3 | **Документация и верификация.** README ×2 (сборка с feature, список с пометкой, ограничения, раздел «Задержка»), AGENTS.md, PROJECT_MAP, `docs/verify/2026-10-22-asio-backend.md` по К-1…К-9 | `readme_keys`, `i18n_completeness` зелёные; верификация — каждый критерий со ссылкой |

## 4. Инварианты

- Аудио-коллбэк без аллокаций и блокировок (N-03): конвертация формата — поэлементная запись в уже выделенный cpal буфер.
- Индексы устройств хоста по умолчанию не меняются: ASIO-устройства идут после них.
- Сборка без feature: ни одной новой зависимости в компиляции, `--list-devices` и F3 — как раньше.
- Строки UI — только через словарь; пометка `[ASIO]` — часть имени устройства.
