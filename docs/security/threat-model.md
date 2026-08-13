# EvoHime — security threat model

Дата: 2026-08-04. Версия клиента: `0.0.0001`.

## Граница доверия

Ева работает локально на Windows-машине пользователя. `EvoHime.exe` отображает состояние, `evohime-core.exe` выполняет агентный цикл и tools, `evohime-supervisor.exe` контролирует жизненный цикл. UI и Core связаны bounded versioned named pipe; Core — единственный владелец workspace и SQLite.

## Защищаемые операции

| Область | Контроль |
| --- | --- |
| Workspace | нормализация пути, sandbox и запрет traversal |
| Filesystem/Git | permissions, approval и preview до изменения |
| Shell | allowlist окружения, timeout, cancellation, ограничение stdout/stderr |
| Child processes | Windows Job Object и завершение дерева при Stop/exit |
| Credentials | Credential Manager/DPAPI, redaction в logs/events |
| IPC | current-user pipe, major/minor compatibility, bounded frames |
| Storage | SQLite WAL, transactional migrations, backup перед upgrade |
| Recovery | event journal, sequence replay, supervisor restart budget |
| External tools | отдельные permission scopes, host/path validation и approval |

## Основные угрозы

- prompt injection в тексте workspace или model context;
- опасная tool-команда, замаскированная под безопасную;
- выход инструмента за пределы workspace;
- утечка API key в task event, log или diagnostics;
- зависший shell-процесс после остановки задачи;
- повреждение SQLite во время миграции или сбоя питания;
- подмена/несовместимость IPC-команды;
- вредоносный plugin или внешний MCP endpoint.

## Ограничения

Скомпрометированная Windows-машина, права администратора и доверенный пользователь вне scope. Multi-user deployment и удалённое управление не являются целями первого клиента.

## Проверка перед релизом

Публикация установщика разрешена только после Rust tests, Electron tests, package smoke, `git diff --check` и успешной Windows package compilation на Windows CI.
