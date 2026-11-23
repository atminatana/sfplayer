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

## Шаг 1.2 (2026-11-23) — строка «Выход»

| Тест | Файл | Что проверяет |
| --- | --- | --- |
| `a_name_longer_than_the_field_loses_its_end_in_the_field_and_in_the_list` | `src/ui/controls/tests.rs` | имя длиннее поля обрезано с `…` в поле и в строке раскрытого списка; отметка поля на месте; выбранное значение найдено в списке по полному имени |
| `the_output_field_has_room_for_an_exclusive_output_and_what_follows_it` | `src/ui/main_screen/tests.rs` | на сетке 96×36 поле вмещает `[WASAPI Exclusive]: ` и имя в 27 знаков, за ним — частота, размер коллбэка и `OK` |
| `an_output_name_longer_than_the_field_loses_its_end` | там же | имя длиннее поля обрезано с `…`, остальная строка на месте |
| `the_output_row_names_the_rate_alone_when_the_driver_does_not_tell_the_size_of_a_callback` | там же | без размера коллбэка после частоты сразу `OK` |
| `an_error_of_the_output_stays_inside_the_frame` | там же | длинная ошибка обрезана перед отступом рамки, строка не длиннее сетки |

Изменены ожидания существующих тестов `main_screen/tests.rs`: выход в образце — `[WASAPI]: Speakers`, подпись — `48000 Hz / 256  OK` (было `Speakers`, `48000 Hz / 256 кадров  OK`): это новый вид строки по ТЗ D-01, D-06.

Прогон после шага: 414 unit + 3 i18n + 4 no_alloc + 2 readme = 423 passed, 0 failed.

**Ограничения:** настоящий размер коллбэка в окне автотестом не проверяется — нужен поток на устройстве.

## Шаг 2.1 (2026-11-23) — перевод f32 → PCM

| Тест | Файл | Что проверяет |
| --- | --- | --- |
| `silence_is_zero_and_full_scale_is_the_largest_value_of_the_format` | `src/audio/pcm.rs` | тишина — ноль; `1.0` и `-1.0` — наибольшее и наименьшее значения 16 и 24 бит |
| `a_sample_is_rounded_to_the_nearest_step` | там же | половина и четверть шкалы; меньше половины шага — тишина, больше — один шаг |
| `what_is_beyond_full_scale_saturates_and_not_a_number_is_silence` | там же | значения за ±1 и бесконечности насыщаются, не-число — ноль |
| `a_24_bit_sample_takes_three_bytes_low_byte_first` | там же | порядок байтов 24-битного сэмпла и его отрицания |
| `only_as_many_samples_are_written_as_the_output_has_room_for` | там же | лишние сэмплы не пишутся, неполный хвост выходного буфера не трогается |
| `the_formats_are_asked_for_best_first` | там же | порядок опроса: 24 бита, затем 16; размер сэмпла в байтах |

Прогон после шага: 420 unit + 3 i18n + 4 no_alloc + 2 readme = 429 passed, 0 failed.
