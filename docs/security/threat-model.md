# EvoHime — security threat model

Дата: 2026-08-25. Пользовательские версионные релизы для текущего цикла не создаются.

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
| IPC | owner-only DACL на pipe, непредсказуемое имя endpoint, одноразовый nonce и HMAC-proof из launch context, OS-идентичность клиента, major/minor compatibility, bounded frames |
| Storage | SQLite WAL, transactional migrations, backup перед upgrade |
| Recovery | event journal, sequence replay, supervisor restart budget |
| External tools | отдельные permission scopes, host/path validation и approval |
| Self-repair/update | user-only FSM, isolated canonical checkout, protected-path gate, separate commit/push approvals, CI check and post-restart health rollback |

## Основные угрозы

- prompt injection в тексте workspace или model context;
- опасная tool-команда, замаскированная под безопасную;
- выход инструмента за пределы workspace;
- утечка API key в task event, log или diagnostics;
- зависший shell-процесс после остановки задачи;
- повреждение SQLite во время миграции или сбоя питания;
- подмена/несовместимость IPC-команды;
- вредоносный plugin или внешний MCP endpoint.
- подмена исходного репозитория, изменение release/security-контура через
  repair-run или публикация непроверенного commit;
- успешная подмена файлов установки при том, что новая версия не подняла
  authenticated Core.

## Аутентификация desktop IPC

Supervisor создаёт один launch context на сессию: непредсказуемое имя pipe,
session secret и ожидаемую Windows-идентичность (user SID, logon session).
Контекст лежит в каталоге с DACL только для владельца
(`%LOCALAPPDATA%\EvoHime\runtime\session.json`) и удаляется вместе с сессией.

Core создаёт pipe сам, с protected DACL `D:P(A;;GA;;;<user SID>)`, и на каждое
подключение выдаёт одноразовый nonce с ограниченным временем жизни. Клиент
отвечает handshake с ролью и `HMAC-SHA256(secret, role | client_id | nonce)`.
Core отвергает несовместимый major, чужую идентичность (берётся у ОС через
impersonation, а не из слов клиента), неизвестную роль, просроченный,
повторно использованный или не совпавший nonce и неверный proof. Имя pipe
считается непредсказуемым, но не секретом: защиту дают ACL и handshake.

Роли транспорта: `shell` — Electron-оболочка, `compatibility-shell` — WinUI
compatibility runtime. Core без launch context (запуск разработчика без
supervisor) работает в неаутентифицированном режиме и явно помечает это в
`core.started` и в логе соединения.

**Что это не защищает.** Модель угроз считает текущего пользователя доверенным.
ACL и session binding закрывают доступ другого пользователя и другой logon
session, но не дают гарантий против вредоносного кода, уже выполняющегося с
правами этого же пользователя: он может прочитать launch context и
подключиться как оболочка. Защита от такого сценария требует отдельных
механизмов уровня ОС и в первый клиент не входит.

## Ограничения

Скомпрометированная Windows-машина, права администратора и доверенный пользователь вне scope. Multi-user deployment и удалённое управление не являются целями первого клиента.

## Проверка перед релизом

Публикация текущего установщика разрешена только после Rust tests, Electron
typecheck/tests, protocol и bundle checks, deterministic evaluation и security
gate, WinUI/IPC compatibility tests, package smoke, `git diff --check` и
успешной Windows package compilation на Windows CI. Release job дополнительно
выполняет startup/fault acceptance, install/upgrade/rollback smoke, repair
protected-path/health-marker tests и проверку retention.
