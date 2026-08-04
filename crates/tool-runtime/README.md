# evohime-tool-runtime

Локальный runtime инструментов для `evohime-core`.

Поддерживаемые инструменты включают `filesystem.read`, `filesystem.write`, `filesystem.patch`, `filesystem.search`, `shell.execute`, Git, MCP и дополнительные browser/http adapters. Все вызовы проходят через workspace sandbox, permissions, cancellation, timeout и approval gate.

Runtime не является веб-сервисом и не зависит от browser UI. WinUI только отображает события Core; бизнес-логика инструментов остаётся в Rust.
