# Верификация: опциональный бэкенд ASIO (v1.2)

- **id:** `2026-10-22-asio-backend`
- **ТЗ:** `docs/spec/2026-10-22-asio-backend.md`; **план:** `docs/plans/2026-10-22-asio-backend.md`
- **Дата:** 2026-10-22
- **Метод (BP-09):** для каждого критерия ТЗ §6 открыт код, тест или вывод команды; прогоны 22.10.2026 на машине владельца (Windows 10, LLVM 21.1.1, VS 18, драйвер `Realtek ASIO`). Статусы: ✓ — подтверждено кодом и автотестом/командой; ◐ — код есть, автоматическая проверка невозможна; ✗ — не подтверждено, требуется действие владельца.

## Критерии приёмки (ТЗ v1.2 §6)

| # | Критерий | Статус | Подтверждение |
|---|---|---|---|
| К-1 | fmt / clippy / test без feature зелёные; существующий CI зелёный | ✓ / ◐ | прогон после шага 3: fmt ok, clippy 0 предупреждений, 178 unit + 3 + 2 passed; ◐ — CI после пуша |
| К-2 | clippy и test с `--features asio` зелёные на Windows | ✓ | `cargo clippy --all-targets --features asio -- -D warnings` чисто; `cargo test --features asio` 178 unit (1 ignored) + 3 + 2 passed (шаг 2) |
| К-3 | `--list-devices`: с feature — WASAPI с прежними индексами, затем `Realtek ASIO [ASIO]` с частотой и буфером; без feature — как раньше | ✓ | с feature: `* [0] Динамики (Realtek(R) Audio)  48000 Hz, buffer 480..480 frames`, `[1] Realtek ASIO [ASIO]  48000 Hz, buffer 1024..1024 frames`; без feature — только `[0]`; `device.rs::label_outputs` + тесты |
| К-4 | TUI с `--audio-out "Realtek ASIO"`: строка F3 `OK`, нота звучит, в логе формат и буфер | ◐ / ✗ | открытие потока подтверждено живым тестом `asio_driver_opens_a_stream_in_its_own_format` (`format=i24 sample_rate=48000 buffer=Default`); звук и строка F3 — ✗ `[data needed: прогон владельцем: cargo run --features asio -- --audio-out "Realtek ASIO" --sf2 <банк>; в логе строка "audio stream started" с backend=asio sample_format=i24]` |
| К-5 | F3 перечисляет и переоткрывает ASIO ↔ WASAPI; `audio.output` с пометкой; после перезапуска то же устройство | ✗ | `[data needed: интерактивно: F3 → `Realtek ASIO [ASIO]` → Enter, затем обратно; проверить `output = "Realtek ASIO [ASIO]"` в config.toml и перезапуск]`; код: `output_device_by_index` по общему списку, `reopen_audio` закрывает старый поток до открытия нового (`session.rs`); риск — cpal прерывает перечисление ASIO на `DriverAlreadyExists`, пока поток держит драйвер |
| К-6 | конвертация f32 → i16/i32/I24/f64 покрыта unit-тестами | ✓ | `render/tests.rs::interleave_converts_the_f32_render_to_the_output_sample_type`, `integer_and_f64_outputs_are_silent_without_an_engine`, `a_note_reaches_an_i32_output` |
| К-7 | объединение списков без cpal: индексы, пометка, `*` | ✓ | `device.rs::default_host_devices_keep_their_order_and_asio_ones_follow_tagged`, `selector_reaches_asio_devices_by_tag_name_or_index`, `only_the_default_host_default_device_is_marked` |
| К-8 | README ×2 синхронны; `readme_keys`, `i18n_completeness` зелёные | ✓ | правки в пяти одинаковых местах обоих файлов; `cargo test` после шага 3 — оба теста passed |
| К-9 | замер задержки N-02 с ASIO | ✗ | `[data needed: измерение владельцем по методике README «Задержка»]`; ожидание по данным драйвера: буфер 1024 кадра при 48 000 Гц ≈ 21 мс только на буфер — цель 15 мс этим драйвером недостижима |

## Решения владельца (ТЗ §7)

| # | Решение | Где применено |
|---|---|---|
| ОР-1 | единый список с пометкой `[ASIO]`, без поля/флага бэкенда | ADR 0012; `audio/device.rs::enumerate_outputs`, `label_outputs`, `OutputDevice.name` |
| ОР-2 | шаг CI с feature, advisory | `.github/workflows/ci.yml` «Clippy with the asio feature (Windows, advisory)» |
| ОР-3 | релизы без ASIO | `release.yml` не менялся; README «Ограничения» |

## Расхождения и уточнения

- ТЗ A-07 предполагал, что буфер `Fixed(256)` может быть принят; `Realtek ASIO` принимает только 1024 — сработал предусмотренный fallback на `Default`, расхождения с ТЗ нет, но цель N-02 на этом драйвере не достигается (К-9).
- ТЗ §3 не знал, что драйвер может сообщать частоту 0 до старта потока; добавлена запасная частота (`pick_sample_rate`, шаг 2) — ТЗ A-07 выполняется с этим дополнением, ошибка вместо паники.
- Сборка с feature при llvm-mingw первым в `PATH` требует `CLANG_PATH` — отражено в README и CI.
