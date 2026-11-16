# Архитектурные решения (ADR)

Формат: Дата / Статус / Тема → Контекст → Решение → Альтернативы и обоснование → Последствия.

| № | Тема | Статус |
|---|------|--------|
| [0001](0001-synth-engine-rustysynth.md) | Движок синтеза: rustysynth за трейтом SynthEngine | Accepted |
| [0002](0002-threading-lock-free.md) | Потоковая модель: аудио-поток без блокировок, SPSC-кольца и атомики | Accepted |
| [0003](0003-windows-wasapi-shared.md) | Аудио на Windows: только WASAPI shared, без ASIO | Accepted |
| [0004](0004-hotkeys-no-letters.md) | Горячие клавиши без букв: F-клавиши и Ctrl/Alt | Accepted |
| [0005](0005-recording-flac-vorbis.md) | Запись в FLAC / Ogg Vorbis, кодирование в отдельном потоке | Accepted |
| [0006](0006-package-name-sf2-player.md) | Имя пакета и бинарника: sf2_player | Accepted |
| [0007](0007-config-keyboard-and-keys.md) | Секции конфига: [keyboard] для нот, [keys] для команд | Accepted |
| [0008](0008-i18n-from-first-tui-line.md) | Локализация rust-i18n с первой строки TUI | Accepted |
| [0009](0009-text-keyboard-via-command-ring.md) | События текстовой клавиатуры через кольцо команд UI→audio | Accepted |
| [0010](0010-settings-list-screen.md) | Главный экран — список настроек; монитор MIDI и полосы каналов убраны (v1.1) | Accepted |
| [0011](0011-global-preset.md) | Пресет F5 — один на все каналы, кроме ударных (v1.1) | Accepted |
| [0012](0012-gterm-window-ui.md) | Интерфейс — окно gterm вместо терминала; Windows и Linux; уточнение о лицензиях (v1.2) | Accepted |
| [0013](0013-builtin-synth-shared-effects.md) | Встроенный синтезатор — второй движок за `SynthEngine`; эффекты с общим управлением (v1.2) | Accepted |
| [0014](0014-smf-reader-and-sequencer-thread.md) | MIDI-файл: собственный разбор SMF и поток-секвенсор со своим кольцом (v1.2) | Accepted |
| [0015](0015-midi-take.md) | Запись MIDI: события из аудио-потока с меткой по счётчику кадров, файл SMF формата 0 (v1.2) | Accepted |
| [0016](0016-dependency-licences.md) | Лицензии зависимостей: MPL-2.0 без правок и Unicode-3.0 допустимы (v1.3) | Accepted |
| [0017](0017-split-large-files.md) | Крупные файлы делятся на подмодули по ответственности (v1.3) | Accepted |
