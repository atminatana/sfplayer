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
