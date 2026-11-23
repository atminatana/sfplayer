# Отчёт о тестах: драйверы аудиовыхода — WASAPI, WASAPI Exclusive, ASIO (v1.4)

- **id:** `2026-11-23-audio-drivers`
- Запуск: `cargo test`; формат и линт: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.

Разделы добавляются по шагам плана.

## Базовый прогон (2026-11-23)

До первой правки, на `c5230ab`: 404 unit + 3 i18n + 4 no_alloc + 2 readme = 413 passed, 0 failed; формат и линт чистые.

## Замеры до правок (2026-11-23)

Измеритель вне репозитория (`%LOCALAPPDATA%\Temp\sfplayer_latency_probe`), тишина 1,5 с на вариант, машина владельца.

| Выход | Режим | Буфер | Период коллбэка | «Коллбэк → вывод» |
| --- | --- | --- | --- | --- |
| Динамики (Realtek(R) Audio), 48 кГц | WASAPI shared, `Fixed(256)` / `Fixed(128)` / `Default` | 1056 кадров (22 мс) | 480 кадров (10 мс) | 32 мс |
| Наушники (2- QCY H3 Stereo), 44,1 кГц | то же | 970 кадров (22 мс) | 441 кадр (10 мс) | 32 мс |

`IAudioClient3`: минимальный период движка у обоих выходов — 10 мс (равен обычному). Монопольный режим, только опрос: минимальный период 3 мс у обоих; PCM 16 бит принимают оба, 24 бита — только Realtek.

## Шаг 1.1 (2026-11-23) — драйвер и единый список

| Тест | Файл | Что проверяет |
| --- | --- | --- |
| `an_output_is_named_by_its_driver_and_its_device` | `src/audio/device.rs` | имя выхода — `[Драйвер]: Имя устройства` для каждого драйвера |
| `a_device_name_from_an_old_config_finds_the_output_of_the_host_driver` | там же | имя устройства без драйвера находит выход драйвера ОС, без учёта регистра |
| `a_whole_label_finds_the_output_of_its_own_driver` | там же | полное имя и название драйвера находят выход своего драйвера |
| `an_index_counts_the_outputs_of_every_driver` | там же | индекс считает строки всех драйверов; индекс за концом списка ничего не находит |
| `only_the_default_device_of_the_host_driver_is_marked_as_the_default` | там же | отметка умолчания — только у устройства по умолчанию драйвера ОС |
| `audio_rows_name_the_driver_and_the_device_and_choosing_one_opens_the_output_by_its_index` | `src/ui/devices.rs` | строки выпадающего списка — полные имена; выбор строки открывает выход по его индексу (был `audio_rows_are_the_device_names_…`) |

Прогон после шага: 409 unit + 3 i18n + 4 no_alloc + 2 readme = 418 passed, 0 failed.

**Ограничения:** перечисление настоящих устройств (`enumerate_outputs`, `list_output_devices`) автотестом не покрыто — нужно устройство; проверено живым `--list-devices` (отчёт о выполнении, шаг 1.1).
