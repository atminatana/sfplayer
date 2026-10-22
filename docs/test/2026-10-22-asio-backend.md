# Отчёт о тестах: опциональный бэкенд ASIO (v1.2)

- **id:** `2026-10-22-asio-backend`
- Запуск: `cargo test` (шаг 2 — дополнительно `cargo test --features asio`); формат и линт: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.

Разделы добавляются по шагам плана.

## Шаг 1 (2026-10-22)

**Состав**

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `default_host_devices_keep_their_order_and_asio_ones_follow_tagged` | `audio/device.rs` | объединение списков: индексы хоста по умолчанию прежние, ASIO после них с пометкой ` [ASIO]`, устройства без имени получают `device N` / `asio device N` (К-7) |
| `selector_reaches_asio_devices_by_tag_name_or_index` | `audio/device.rs` | `--audio-out` находит ASIO-устройство по подстроке `[asio]`, по имени драйвера и по индексу; WASAPI-имя по-прежнему первым (К-7) |
| `only_the_default_host_default_device_is_marked` | `audio/device.rs` | `*` только у умолчания хоста по умолчанию, не у ASIO с тем же именем (К-7) |
| `backend_labels_are_stable_log_tokens` | `audio/device.rs` | метки `default`/`asio` для лога, `Backend::default()` |
| `integer_and_f64_outputs_are_silent_without_an_engine` | `audio/render/tests.rs` | `fill` без движка пишет `EQUILIBRIUM` в i16/i32/I24/f64 (К-6) |
| `interleave_converts_the_f32_render_to_the_output_sample_type` | `audio/render/tests.rs` | крайние значения ±1.0 насыщаются, 0.5/0.25 точны для i16/i32/f64, гейн до конвертации, моно — среднее, лишние каналы — тишина (К-6) |
| `a_note_reaches_an_i32_output` | `audio/render/tests.rs` | нота из кольца слышна в i32-потоке, измеритель читает f32-рендер (К-6) |
| `audio_dialog_describes_rate_and_buffer_range`, `dialog_overlay_shows_title_items_marker_and_detail` (обновлены) | `ui/dialog.rs`, `ui/screen/tests.rs` | конструкторы с полем `backend`; поведение диалога не изменилось |

**Результат** — 177 unit + 3 i18n + 2 readme = 182 passed, 0 failed; unit-прогон 0.78 s.

**Ограничения** — перечисление реального хоста ASIO и открытие потока в формате драйвера в тестах не воспроизводятся (нужны драйверы и feature); покрыты чистые части (`label_outputs`, `is_default_output`, `interleave`, `fill<T>`). Живая проверка — шаг 2.

## Шаг 2 (2026-10-22)

**Состав**

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `unreported_sample_rate_falls_back_to_a_preferred_supported_one` | `audio/device.rs` | сообщённая частота главнее; при 0 — 48 000, затем 44 100, затем максимум из поддерживаемых; пусто или только 0 → `None` |
| `asio_driver_opens_a_stream_in_its_own_format` (`#[ignore]`, `cfg(all(windows, feature = "asio"))`) | `audio/stream.rs` | первый ASIO-драйвер из общего списка открывает поток: имя с пометкой, формат драйвера, `play()` без ошибки (К-4 без TUI) |

**Результат** — без feature: 178 unit + 3 i18n + 2 readme passed, 0 failed; с feature: 178 unit + 3 + 2 passed, 0 failed; живой тест: 1 passed (`format=i24 sample_rate=48000 buffer=Default` на `Realtek ASIO`).

**Живые прогоны** — `--list-devices` с feature: WASAPI `[0]` с прежним индексом, `[1] Realtek ASIO [ASIO]  48000 Hz, buffer 1024..1024 frames`; без feature — только `[0]`. Первый запуск до правки частоты: `0 Hz` в списке и паника `ringbuf` при открытии (ёмкость кольца 0) — исправлено в этом шаге.

**Ограничения** — звук из TUI (К-4), диалог F3 и `audio.output` (К-5), замер задержки (К-9) — только интерактивно, за владельцем; живой тест пропускается без `--ignored` и не выполняется в CI.

## Шаг 3 (2026-10-22)

Новых тестов нет. Прогон после правок документации: `cargo fmt --check` ok, `cargo clippy --all-targets -- -D warnings` чисто, `cargo test` — 178 unit + 3 i18n + 2 readme passed, 0 failed (`readme_keys`: таблицы клавиш не менялись; `i18n_completeness`: в коде нет новых строк UI — пометка `[ASIO]` часть имени устройства, Д-1 ТЗ).

**Синхронность README** — вхождения `[ASIO]`: README.md — 3, README.ru.md — 3.
