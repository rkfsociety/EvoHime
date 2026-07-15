# Milestone 2 — Этап 3: Tools, shell, permissions

## Цель

Довести вертикальный срез EvoHime до безопасной работы агента с файлами и shell-командами через браузер: операции проходят через единый реестр инструментов, опасные действия останавливаются до подтверждения пользователя, а вывод shell отображается в Terminal-панели.

## Границы

В этап входят `filesystem.write`, `filesystem.patch`, `filesystem.search`, `shell.execute`, движок разрешений, approval-события и команды, таймауты/отмена, Terminal-панель, настройки разрешений и регрессионные тесты. Реальный LLM orchestration и новые Git-инструменты этапа 4 не меняются.

## Архитектура

### 1. Tool runtime

`ToolContext` получает общий `WorkspaceSandbox`, который канонизирует пути и запрещает выход за `WORKSPACE_ROOT`, включая symlink-выход. Все filesystem-инструменты используют этот слой.

Инструменты:

- `filesystem.write`: принимает относительный `path` и UTF-8 `content`, создаёт только родительские каталоги внутри workspace, возвращает `created`/`updated` и размер.
- `filesystem.patch`: принимает относительный `path` и unified diff для одного файла; проверяет базовое содержимое, применяет hunk-ы без shell, атомарно записывает результат и возвращает число применённых hunk-ов.
- `filesystem.search`: принимает текстовый `query`, необязательный относительный `path`, glob и лимит; выполняет `rg` как дочерний процесс с рабочей директорией workspace, запрещает аргументы, позволяющие сменить root, и возвращает структурированные совпадения.
- `shell.execute`: принимает команду, аргументы, cwd и timeout; запускает процесс только внутри workspace, с ограничением cwd и без shell-обёртки, возвращает stdout/stderr/exit code. Команда получает отдельное разрешение `ShellExecute`.

Каждый инструмент остаётся зарегистрированным через `ToolDefinition`, имеет timeout и список permissions. Registry проверяет permission decision перед запуском; при `NeedsApproval` возвращает типизированную ошибку с approval id.

### 2. Permissions и approval

`crates/permissions` предоставляет:

- `PermissionPolicy` с режимами `ask`, `allow`, `deny` для каждого permission;
- `PermissionEngine::check(permission, scope)`;
- одноразовые approval-запросы с UUID, task/tool/scope и состоянием pending/granted/denied;
- потокобезопасное хранилище pending approvals в памяти процесса.

Безопасные чтение/поиск могут быть разрешены политикой по умолчанию. Запись и shell требуют approval по умолчанию. Grant действует только для конкретного запроса; постоянные настройки меняются отдельным UI/API действием и не обходят sandbox.

### 3. Protocol и server

В JSON Schema и Rust protocol добавляются:

- `approval.required` с `approval_id`, `task_id`, `tool_name`, permission, scope и created_at;
- client-команды `approval.granted` и `approval.denied` с `approval_id`.

После команды сервер обновляет `PermissionEngine`, публикует результат в session bus и возобновляет ожидающий tool task. Невалидный или просроченный approval не падает весь WebSocket: сервер возвращает `task.failed`/action log с безопасной причиной.

### 4. Frontend

Terminal становится рабочей панелью: approval modal появляется при `approval.required`, показывает инструмент и ограниченный scope, а кнопки отправляют соответствующие WebSocket-команды. Вывод `tool.output` для `shell.execute` отображается в моноширинном терминале с разделением stdout/stderr и кодом возврата.

Settings получает список permission-политик с режимами `ask/allow/deny`, сохраняет их через серверный endpoint/session command и показывает текущий режим. UI не выполняет filesystem/shell-логику самостоятельно.

## Ошибки и безопасность

- Любой путь проверяется после canonicalize; несуществующий путь для записи проверяется через canonicalize родителя.
- Shell запускается без `cmd /c`/`sh -c`; argv передаются напрямую, cwd ограничен workspace.
- Размеры входа/вывода ограничены, чтобы один файл или процесс не блокировал сервер.
- Timeout завершает дочерний процесс, cancellation использует `CancellationToken` и не оставляет approval в неопределённом состоянии.
- Ошибки permission, path traversal, invalid diff, timeout и non-zero exit code покрываются отдельными тестами.

## Порядок поставки

1. Sandbox, filesystem write/patch/search, shell и registry permission gate.
2. Permission engine, protocol schema/codegen, server approval lifecycle.
3. Terminal, modal, настройки и end-to-end smoke flow.

Каждая часть должна иметь Rust/TypeScript тесты и проходить `cargo test`, protocol generation и `frontend/web npm run build`.

## Критерий готовности

В тестовом workspace агент может безопасно прочитать, записать, пропатчить и найти текст; shell-команда выполняется только внутри workspace и возвращает вывод; опасная операция генерирует `approval.required`, UI отправляет grant/deny, а разрешённый shell-output виден в Terminal-панели.
