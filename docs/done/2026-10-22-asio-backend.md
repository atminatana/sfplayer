# Отчёт о выполнении: опциональный бэкенд ASIO (v1.2)

- **id:** `2026-10-22-asio-backend`
- **План:** `docs/plans/2026-10-22-asio-backend.md`
- **ПОЧЕМУ** решений — `docs/adr/0012-asio-optional-feature.md`, здесь только WHAT.

Разделы добавляются по мере завершения шагов плана.

## Шаг 1 — ядро без feature: единый список устройств, поток в формате драйвера (2026-10-22)

**Сделано**

- `Cargo.toml`: feature `asio = ["cpal/asio"]`, не в `default`.
- `src/audio/device.rs`: `Backend { Default, Asio }` с `label()`, `OutputDevice { device, backend, name }`, одно перечисление `enumerate_outputs()` = хост по умолчанию + `asio_output_devices()` (при `cfg(all(windows, feature = "asio"))` — хост `HostId::Asio`, ошибки хоста → `warn` в лог и пустой список; иначе пустая заглушка); чистая часть `label_outputs` (порядок, имена без описания → `device N`, пометка ` [ASIO]`) и `is_default_output` (`*` только у умолчания хоста по умолчанию); `list_output_devices`, `select_output_device`, `output_device_by_index`, `default_output_device` поверх перечисления; `AudioDeviceInfo.backend`.
- `src/audio/render.rs`: `fill<T: cpal::Sample + FromSample<f32>>` и `interleave<T>` — тишина `T::EQUILIBRIUM`, выход через `T::from_sample`; рендер, измеритель и запись остаются f32.
- `src/audio/stream.rs`: `open_output_stream(&OutputDevice, …)` берёт `sample_format()` устройства и строит поток через `build_stream::<f32 | i16 | i32 | I24 | f64>`; другой формат → `BuildStream` с текстом cpal; лог старта дополнен `backend` и `sample_format`.
- `src/audio/mod.rs`: экспорт `Backend`, `OutputDevice`; `src/session.rs`: `open_audio(&OutputDevice)`.
- Тесты диалога F3 и экрана: поле `backend` в конструкторах `AudioDeviceInfo`.
- `docs/adr/0012-asio-optional-feature.md` (Accepted), ADR 0003 → Superseded by 0012 для сборок с feature, индекс ADR.

**COMPLETENESS** — ТЗ A-01 (feature): `Cargo.toml`; A-02/A-03/A-05 (единый список, пометка, одно перечисление): код + тесты `label_outputs`/селектора; A-04 (без feature ASIO-строк нет, ошибки хоста в лог): заглушка + `warn`-ветки, тест невозможен без ASIO (живая проверка — шаг 2); A-06 (формат драйвера, конвертация без аллокаций): код + тесты `interleave`/`fill` для i16/i32/I24/f64; A-08 (ошибки в `AudioError::*`): `BuildStream`/`DeviceConfig` как раньше; A-09 (лог): `backend`, `sample_format`; A-13 (ADR): 0012 + индекс. A-07, A-10, A-11, A-12, A-14 — шаги 2–3.

**CORRECTNESS** — индексы устройств хоста по умолчанию не меняются (ASIO-записи дописываются после); `*` не ставится ASIO-устройству даже при совпадении имени; конвертация f32→int насыщается на полной шкале (32 767 / −32 768), гейн применяется до конвертации, моно — среднее, лишние каналы — тишина; f32-путь WASAPI не изменился (`T = f32` — тождественная конвертация); существующие тесты render/dialog/screen/session зелёные без правок ожиданий.

**EVIDENCE** — `cargo fmt --check` ok; `cargo clippy --all-targets -- -D warnings` чисто; `cargo test`: 177 unit + 3 i18n + 2 readme = 182 passed, 0 failed (baseline на `develop` до изменений — 170 unit + 5). Новых тестов 7 (4 в `device.rs`, 3 в `render/tests.rs`). LOC после правок: `session.rs` 431, `render/tests.rs` 386, `render.rs` 261, `device.rs` 244, `stream.rs` 161 — ниже порогов QG-06. Верификационный проход: diff всех 11 файлов перечитан, ложных правок 0.

**⚠ WARNINGS**

- Ветка `asio_output_devices` под `cfg(all(windows, feature = "asio"))` на этом шаге не компилировалась — проверка со feature и живой прогон на `Realtek ASIO` — шаг 2.
- A-04 «ошибка хоста ASIO → только лог» проверяется только живым прогоном (нет способа подменить хост cpal в тестах).
- `Cargo.lock` на этом шаге не изменён (feature не включался); после первой сборки с feature в нём появятся `asio-sys` и сборочные зависимости.

## Шаг 2 — сборка с feature, живая проверка, CI (2026-10-22)

**Сделано**

- `src/audio/device.rs`: `device_sample_rate` + чистая `pick_sample_rate` — если устройство сообщает частоту 0 (Realtek ASIO до старта потока — задокументировано в cpal `host/asio/device.rs:189`), берётся первая из `PREFERRED_SAMPLE_RATES` (48 000, 44 100), которую драйвер принимает, иначе максимальная поддерживаемая; `--list-devices` и диалог F3 показывают её вместо `0 Hz`.
- `src/audio/stream.rs`: частота 0 никогда не доходит до колец — `AudioError::DeviceConfig` «the device reports no usable sample rate» вместо паники `ringbuf` при ёмкости 0; живой тест `asio_driver_opens_a_stream_in_its_own_format` (`#[ignore]`, только `cfg(all(windows, feature = "asio"))`): первый ASIO-драйвер открывает поток в своём формате.
- `.github/workflows/ci.yml`: шаг «Clippy with the asio feature (Windows, advisory)» — `if: runner.os == 'Windows'`, `continue-on-error: true`, `LIBCLANG_PATH`/`CLANG_PATH` на предустановленный LLVM образа.
- `Cargo.lock`: `asio-sys 0.4.0`, `bindgen 0.72.1`, `clang-sys`, `cexpr`, `libloading`, `prettyplease`, `rustc-hash`, `shlex` (компилируются только с feature).

**Окружение сборки (для README, шаг 3).** Первая сборка упала в `bindgen`: на `PATH` первым стоит `clang` из llvm-mingw, его встроенные заголовки (`mmintrin.h`) не парсятся в MSVC-режиме; лечится `CLANG_PATH=C:\Program Files\LLVM\bin\clang.exe` и `LIBCLANG_PATH=C:\Program Files\LLVM\bin`. SDK `asio-sys` скачал сам в `%TEMP%\asio_sdk` (там же `Steinberg ASIO Licensing Agreement.pdf`).

**COMPLETENESS** — К-2: clippy и тесты с feature зелёные; К-3: `--list-devices` обеих сборок; К-4 частично: поток на `Realtek ASIO [ASIO]` открывается в формате драйвера (живой тест), звук из TUI — за владельцем; A-07 подтверждён вживую: буфер 256 отвергнут (диапазон драйвера 1024..1024) → fallback `Default`; A-11: шаг CI. К-5 (F3 туда и обратно) — за владельцем.

**CORRECTNESS** — выбор частоты: сообщённая частота всегда главнее; при 0 — предпочтительная из принятых драйвером, не произвольная константа; при отсутствии данных — ошибка, не паника. Живой тест проверяет имя с пометкой, формат из `default_output_config` и успешный `play()`.

**EVIDENCE** — `cargo clippy --all-targets --features asio -- -D warnings` чисто; `cargo test --features asio`: 178 unit + 3 i18n + 2 readme passed, 0 failed; `cargo test --features asio -- --ignored asio_driver_opens --nocapture`: `ASIO live: device=Realtek ASIO [ASIO] format=i24 sample_rate=48000 buffer=Default`, 1 passed; `cargo run --features asio -- --list-devices`: `* [0] Динамики (Realtek(R) Audio)  48000 Hz, buffer 480..480 frames` / `[1] Realtek ASIO [ASIO]  48000 Hz, buffer 1024..1024 frames`; без feature — только строка `[0]`. Без feature: `cargo fmt --check` ok, clippy чисто, `cargo test` 178 unit + 5 passed. YAML `ci.yml` разобран `yaml.safe_load`, env-пути с обратными слэшами проверены. LOC: `device.rs` 286, `stream.rs` 202 — ниже порогов QG-06.

**⚠ WARNINGS**

- К-4 (звук из TUI) и К-5 (диалог F3, `audio.output` с пометкой, перезапуск) требуют интерактивного терминала — `[data needed: прогон владельцем `cargo run --features asio -- --audio-out "Realtek ASIO" --sf2 <банк>` и лог]`.
- cpal при перечислении ASIO останавливается на `DriverAlreadyExists`, если какой-то драйвер уже удерживается активным потоком (`host/asio/device.rs:226`): пока открыт ASIO-поток, список в F3 может не содержать ASIO-устройств — проверяется только вживую (К-5).
- Realtek ASIO принимает только буфер 1024 кадра: при 48 000 Гц это ≈ 21 мс на буфер, цель N-02 (15 мс) этим драйвером не достигается; замер К-9 — за владельцем.
- Первый прогон шага CI с feature — после пуша; образ `windows-latest` должен содержать `C:\Program Files\LLVM` (по документации образов GitHub — содержит), иначе шаг падает, не блокируя остальные.
