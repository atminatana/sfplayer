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
