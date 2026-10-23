# ТЗ: опциональный бэкенд ASIO на Windows (v1.2)

Oct 22, 2026 · черновик агента по постановке владельца; открытые решения ОР-1…ОР-3 приняты владельцем 22.10.2026 (§7) — ТЗ считается утверждённым с этими ответами. Базовое ТЗ — `docs/spec/2026-10-20-sf2-player-v1.md`; этот документ отменяет ответ заказчика «ASIO в v1.0 не нужен» (§9 v1) и ADR 0003 для сборок с feature `asio`; сборка по умолчанию и все остальные разделы v1/v1.1 остаются в силе.

## 1. Цель

Дать пользователю Windows путь к меньшей задержке через ASIO-драйвер звуковой карты, не меняя ничего для остальных: обычная сборка `cargo build --release` по-прежнему не требует SDK и LLVM, релизные артефакты не меняются. ASIO-устройства появляются в обычном списке аудиовыходов только в сборке с feature `asio` и выбираются так же, как любое другое устройство.

Решение владельца (22.10.2026): вариант «опциональный feature, выключен по умолчанию» — из трёх предложенных (всегда включён на Windows; отказ от ASIO).

## 2. Границы

**Входит**

- Cargo-feature `asio` (по умолчанию выключен), пробрасывающий `cpal/asio`.
- Единый список аудиовыходов: устройства хоста по умолчанию, затем ASIO-драйверы с пометкой `[ASIO]` (F-03, F-04, диалог F3, `--list-devices`, `--audio-out`, `audio.output`).
- Открытие потока в формате сэмплов драйвера с конвертацией из f32 в аудио-коллбэке без аллокаций (§4.2).
- Шаг CI для сборки с feature на `windows-latest`.
- Документация: README ×2, ADR 0012 (Supersedes 0003), PROJECT_MAP, AGENTS.md.

**Не входит**

- Поле конфига или флаг CLI для выбора бэкенда — бэкенд определяется выбранным устройством (ОР-1).
- Панель управления ASIO-драйвера; ASIO-вход (MIDI и запись не меняются); WASAPI exclusive; JACK на Windows.
- ASIO в релизных архивах (ОР-3).
- Изменение поведения, формата конфига или CLI для сборки без feature.

## 3. Факты, на которых строится ТЗ (проверены 22.10.2026)

| Факт | Источник |
| --- | --- |
| cpal 0.18.2: `HostId::Asio` и `host_from_id` доступны при `cfg(all(windows, feature = "asio"))`; `default_host()` на Windows — всегда WASAPI | `cpal-0.18.2/src/platform/mod.rs:154–160, 780, 912` |
| ASIO-бэкенд не конвертирует формат: запрошенный `SampleFormat` обязан совпадать с форматом драйвера, иначе `ErrorKind::UnsupportedConfig` | `src/host/asio/stream.rs:495–510`, ветки `(F32, ASIOSTFloat32*)`, `(I32, ASIOSTInt32*)`, `(I16, …)`, `(I24, …)`, `(F64, …)` |
| `BufferSize::Fixed` принимается в пределах `buffersize_range()` драйвера, иначе ошибка; частота — `can_sample_rate` + `set_sample_rate` | `src/host/asio/stream.rs:869–913, 1118–1160` |
| `cpal::SizedSample` реализован для i16, i32, f32, f64, `I24`; конвертация через `cpal::FromSample<f32>` (dasp_sample) | `src/sample_format.rs:257–271`, `src/lib.rs:194` |
| Ядро коллбэка `BlockRenderer::fill` режет буфер на чанки `MAX_CHUNK_FRAMES`, рендерит в свои `left`/`right` и интерливит в выходной `&mut [f32]`; измеритель и запись читают `left`/`right` | `src/audio/render.rs:114–140` |
| Сборка `asio-sys`: libclang (`LIBCLANG_PATH`), MSVC (`vcvarsall.bat`), ASIO SDK по `CPAL_ASIO_DIR` или автозагрузка с сайта Steinberg во временный каталог | README cpal, раздел «Compiling for ASIO» |
| На машине владельца: libclang `C:\Program Files\LLVM\bin`, Visual Studio 18 Community, один ASIO-драйвер — `Realtek ASIO` (`HKLM\SOFTWARE\ASIO`) | команды `ls`, `vswhere`, `reg query` |
| Текущий поток: только f32 stereo, `build_output_stream::<f32>`; устройства берутся из `cpal::default_host()` в трёх функциях `audio/device.rs`; диалог F3 и строка F3 показывают `AudioDeviceInfo.name` / `AudioStream.device_name`; `audio.output` сохраняет имя при переоткрытии | `src/audio/stream.rs`, `src/audio/device.rs`, `src/ui/dialog.rs:78–92`, `src/session.rs:184–206` |

## 4. Требования

Приоритеты как в v1: M — обязательно, S — желательно, C — по возможности.

### 4.1 Сборка и список устройств

| № | Требование | Приор. |
| --- | --- | --- |
| A-01 | `Cargo.toml`: feature `asio = ["cpal/asio"]`, не входит в `default`. Сборка без feature не компилирует `asio-sys` и не требует LLVM/SDK (N-09 сохраняется). `Cargo.lock` получит записи `asio-sys` и его сборочных зависимостей — это допустимо | M |
| A-02 | Единая нумерация устройств: сначала выходы хоста по умолчанию в прежнем порядке (индексы не меняются для существующих конфигов), затем устройства хоста ASIO. Пометка по умолчанию `*` — только у устройства по умолчанию хоста по умолчанию | M |
| A-03 | Имя ASIO-устройства в списке, диалоге F3, строке F3, логе и `audio.output` — `<имя драйвера> [ASIO]`; выбор по `--audio-out` / `audio.output` по подстроке или индексу работает по этому имени, как для остальных устройств (F-01/F-03). Поле `AudioDeviceInfo.backend` хранит бэкенд для логики, пометка — для людей | M |
| A-04 | Без feature, не на Windows, при ошибке инициализации хоста ASIO или при отсутствии драйверов ASIO-строк в списке нет; ошибка хоста пишется в лог как `warn` и не становится уведомлением. Один нечитаемый ASIO-драйвер пропускается с `warn`, как сейчас пропускается нечитаемое устройство | M |
| A-05 | Все точки обращения к хосту (`list_output_devices`, `select_output_device`, `output_device_by_index`, `default_output_device`) работают через одно перечисление, чтобы индексы в `--list-devices`, диалоге F3 и `--audio-out INDEX` совпадали | M |

### 4.2 Поток

| № | Требование | Приор. |
| --- | --- | --- |
| A-06 | Поток открывается в формате сэмплов устройства из `default_output_config().sample_format()`: F32 — как сейчас; I16, I32, I24, F64 — коллбэк рендерит в f32 (`left`/`right`) и пишет в выходной буфер через `cpal::FromSample<f32>` без аллокаций (N-03). Измеритель, клип-детектор и запись по-прежнему работают с f32 до конвертации. Другие форматы — `AudioError::BuildStream` с текстом cpal | M |
| A-07 | Буфер и частота: `BufferSize::Fixed(n)` и `--sample-rate` передаются драйверу; отказ драйвера от буфера — существующий fallback на `BufferSize::Default` с предупреждением в логе (F-04); отказ от частоты — существующая ошибка открытия в строке F3 | M |
| A-08 | Ошибки cpal ASIO (`HostUnavailable`, `UnsupportedConfig`, `BackendError`) попадают в существующие `AudioError::*` с именем устройства и причиной | M |
| A-09 | Лог при старте потока дополняется форматом сэмплов и бэкендом (`sample_format`, `backend`) | S |
| A-10 | Счётчик `xrun`: ASIO-бэкенд cpal не сообщает о недогрузах через error callback — счётчик остаётся 0; ограничение фиксируется в README | S |

### 4.3 CI и документация

| № | Требование | Приор. |
| --- | --- | --- |
| A-11 | `ci.yml`: на `windows-latest` дополнительный шаг `cargo clippy --all-targets --features asio -- -D warnings` с `continue-on-error: true` и `LIBCLANG_PATH` на предустановленный LLVM образа; шаг не блокирует остальные проверки (сборка скачивает SDK с сайта Steinberg) | M |
| A-12 | README ×2: раздел сборки с feature (LLVM + `LIBCLANG_PATH`, Visual Studio, `CPAL_ASIO_DIR` или автозагрузка SDK и её лицензионный статус), пометка `[ASIO]` в списке, ограничения (один клиент драйвера, `xrun` не считается, только Windows, релизы без ASIO). Раздел «Задержка» убирает фразу «ASIO не поддерживается» | M |
| A-13 | ADR 0012 «Опциональный бэкенд ASIO: единый список устройств» со статусом Accepted, ADR 0003 — Superseded by 0012; индекс `docs/adr/README.md`; `PROJECT_MAP.md`; AGENTS.md (строка «ASIO не используется») | M |
| A-14 | `docs/plans`, `docs/done`, `docs/test`, `docs/verify` с id `2026-10-22-asio-backend` по TL-01 | M |

## 5. Контракты

```
sf2_player --list-devices
Audio outputs:
* [0] Наушники (2- QCY H3 Stereo)  44100 Hz, buffer range unknown
  [1] Динамики (Realtek(R) Audio)  48000 Hz, buffer range unknown
  [2] Realtek ASIO [ASIO]  48000 Hz, buffer 64..2048 frames

sf2_player --audio-out "Realtek ASIO"      # по подстроке
sf2_player --audio-out 2                    # по индексу
```

```toml
[audio]
output = "Realtek ASIO [ASIO]"   # сохраняется при выборе в F3, как для любого устройства
```

```rust
// src/audio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend { #[default] Default, Asio }
pub struct AudioDeviceInfo { pub index: usize, pub name: String, pub backend: Backend, pub is_default: bool, pub default_sample_rate: u32, pub buffer_range: Option<(u32, u32)> }
/// Устройство с его хостом: имя для людей уже содержит пометку.
pub struct OutputDevice { pub device: cpal::Device, pub backend: Backend, pub name: String }
pub fn list_output_devices() -> Result<Vec<AudioDeviceInfo>, AudioError>;
pub fn select_output_device(selector: Option<&DeviceSelector>) -> Result<SelectedDevice, AudioError>; // SelectedDevice.device: OutputDevice
pub fn output_device_by_index(index: usize) -> Result<OutputDevice, AudioError>;
pub fn open_output_stream(device: &OutputDevice, params: StreamParams, shared: &Arc<AudioShared>) -> Result<(AudioStream, AudioLinks), AudioError>;
// BlockRenderer::fill<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, output: &mut [T], channels: usize)
```

Сигнатуры CLI и конфига не меняются.

## 6. Критерии приёмки

| № | Критерий | Как проверяется |
| --- | --- | --- |
| К-1 | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` зелёные без feature; существующий CI зелёный | команды; CI после пуша |
| К-2 | `cargo clippy --all-targets --features asio -- -D warnings && cargo test --features asio` зелёные на Windows с LLVM и SDK | команды на машине владельца |
| К-3 | `sf2_player --list-devices` в сборке с feature печатает WASAPI-устройства с прежними индексами и затем `Realtek ASIO [ASIO]` с частотой и диапазоном буфера; без feature — список как раньше | живой прогон обеих сборок |
| К-4 | `sf2_player --audio-out "Realtek ASIO" --sf2 <банк>`: строка F3 показывает `Realtek ASIO [ASIO]` и `OK`, нота с текстовой клавиатуры звучит; в логе формат сэмплов драйвера и выбранный буфер | живой прогон + лог |
| К-5 | Диалог F3 в сборке с feature перечисляет и переоткрывает ASIO-устройство и обратно WASAPI; `audio.output` после выбора содержит имя с пометкой, при перезапуске то же устройство выбирается | живой прогон + конфиг |
| К-6 | Конвертация f32 → i16/i32/I24/f64 в `fill` покрыта unit-тестами (крайние значения, тишина без движка, нечётный чанк) | `cargo test` |
| К-7 | Unit-тест перечисления: объединение списков двух хостов — индексы продолжаются, пометка у ASIO-устройств, `*` только у умолчания первого хоста (через тестовую функцию объединения без cpal) | `cargo test` |
| К-8 | README ×2 синхронны; `readme_keys`, `i18n_completeness` зелёные | `cargo test` |
| К-9 | Замер задержки N-02 с ASIO — по методике README; результат заносится в `docs/verify` как измерение или `[data needed]` | владелец, оборудование |

## 7. Открытые решения (AR-04) — приняты владельцем 22.10.2026

- **ОР-1. Как ASIO-устройства попадают в список.** Принято: **единый список с пометкой `[ASIO]`**; без feature или при недоступном хосте ASIO-строк нет; отдельного поля/флага бэкенда нет. Отклонены: переключатель `audio.asio` в конфиге; отдельный бэкенд `audio.backend` + `--audio-backend` с fallback.
- **ОР-2. CI.** Принято: **шаг `cargo clippy --features asio` на `windows-latest`** с `continue-on-error: true` (зависимость от сети и сайта Steinberg). Отклонено: CI без изменений.
- **ОР-3. Релизные артефакты.** Принято: **релизы без ASIO** — бинарник, собранный с ASIO SDK, подпадает под лицензию Steinberg; распространение требует отдельной проверки условий. Отклонено: второй архив `sf2_player-windows-asio`.

## 8. Допущения (локальные, обратимые — BP-02.3)

- Д-1. Пометка `[ASIO]` — часть имени устройства, одинаковая в обеих локалях (имя собственное протокола), в словарь не выносится.
- Д-2. Версия пакета не меняется (0.1.0).
- Д-3. Перечисление устройств ASIO (`default_output_config` на каждом драйвере) может быть медленнее WASAPI — принимается, список запрашивается только при старте, в `--list-devices` и при открытии F3.
- Д-4. Если имя ASIO-драйвера совпадает с именем WASAPI-устройства, подстрока выбирает первое совпадение (WASAPI); ASIO в этом случае выбирается индексом или подстрокой `[ASIO]`.
