# Отчёт о выполнении: драйверы аудиовыхода — WASAPI, WASAPI Exclusive, ASIO (v1.4)

- **id:** `2026-11-23-audio-drivers`
- **План:** `docs/plans/2026-11-23-audio-drivers.md`
- **ПОЧЕМУ** решений — в `docs/adr/0019-audio-output-drivers.md`, здесь только WHAT.

Разделы добавляются по мере завершения шагов плана.

## Шаг 1.1 — драйвер и единый список (2026-11-23)

- `src/audio/device.rs`: `Driver` (`Host`, `WasapiExclusive`, `Asio`) с названием для списка; `output_label` — имя выхода `[Драйвер]: Имя устройства`; `OutputDevice` — устройство с драйвером; одно перечисление `enumerate_outputs` для списка, выбора по подстроке или индексу и выхода по индексу; `AudioDeviceInfo` получил `driver` и `label()`. На этом шаге перечисление отдаёт только выходы драйвера ОС; строки `WASAPI Exclusive` и `ASIO` добавляют шаги 2.2 и 3.1.
- `src/audio/stream.rs`: поток открывается на `OutputDevice`; `AudioStream.label` вместо `device_name` — полное имя выхода; оно же в логе и в текстах ошибок открытия.
- `src/session/devices.rs`, `src/ui/view.rs`: `audio.output`, сообщение о смене выхода и поле строки «Выход» несут полное имя.
- `src/ui/devices.rs`: строки выпадающего списка — полные имена.
- `src/main.rs`: `--list-devices` печатает полные имена.
- `docs/adr/0019-audio-output-drivers.md` (новый), индекс ADR; ADR 0003 — Superseded by 0019.
- `README.md`, `README.ru.md`: вид имени выхода в `--list-devices`, `--audio-out` и `audio.output`.

**COMPLETENESS** — план, шаг 1.1 / ТЗ D-01…D-05, D-07: имя выхода с драйвером во всех пяти местах (список, строка «Выход», лог, `--list-devices`, `audio.output`); порядок и нумерация — одно перечисление; поиск по полному имени; нечитаемое устройство по-прежнему пропускается с `warn`. Тесты — раздел шага в `docs/test/`. D-05 для драйверов, которых ещё нет в перечислении, закрывается на шагах 2.2 и 3.1.

**CORRECTNESS** — живой прогон `cargo run -- --list-devices` на машине владельца: `[0] [WASAPI]: Динамики (Realtek(R) Audio)`, `* [1] [WASAPI]: Наушники (2- QCY H3 Stereo)`, `[2] [WASAPI]: Головной телефон (QCY H3 Hands-Free AG Audio)` — индексы и отметка умолчания те же, что до правки. Значение `audio.output = "Наушники (2- QCY H3 Stereo)"` из действующего конфига находит тот же выход: поиск идёт подстрокой по полному имени (тест `a_device_name_from_an_old_config_finds_the_output_of_the_host_driver`).

**EVIDENCE** — `cargo fmt --check` ok; `cargo clippy --all-targets -- -D warnings` чисто; `cargo test`: 409 unit + 3 i18n + 4 no_alloc + 2 readme = 418 passed, 0 failed (базовый прогон до правок — 413).

**⚠ WARNINGS**

- **Поле строки «Выход» ещё 40 ячеек:** имя с драйвером длиннее и обрезается раньше; ширина и обрезка — шаг 1.2.
- **Открытое решение ОР-4** (дизеринг при переводе в 16 бит) — в плане; шаг 2.1 идёт по допущению Д-5.
- **Имя устройства без названия** в выборе по подстроке теперь `device N`, как в списке (раньше — пустая строка): для устройств с именем поведение не изменилось.
