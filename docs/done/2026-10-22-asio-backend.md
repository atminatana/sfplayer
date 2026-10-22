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
