# План развития Евы по результатам изучения OpenHands

Дата исследования: 11 августа 2026 года.

Исходный проект: [OpenHands/OpenHands](https://github.com/OpenHands/OpenHands).

## Резюме

OpenHands сейчас позиционирует Agent Canvas как локальный или self-hosted центр управления coding-agent задачами. Canvas является frontend-слоем, а выполнение делегируется Agent Server и, при необходимости, Automation Server или внешнему ACP-агенту. Для Евы полезны прежде всего границы ответственности и операционные сценарии OpenHands:

- одна оболочка для разговоров, файлов, терминала, Git, браузера, настроек и автоматизаций;
- переключение между локальными, удалёнными и облачными backends без потери фокуса;
- поток событий с возможностью продолжать разговор и не терять trace;
- подключение внешних агентов через Agent Client Protocol (ACP);
- дочерние агенты для отдельных исследовательских задач;
- skills/plugins/MCP как расширяемая модель инструментов;
- расписания и webhook-события для повторяемых задач;
- явный security-контур вокруг workspace, credentials, sandbox и telemetry.

Переносить React, Python Agent Server, REST/обычный WebSocket или Docker как обязательную основу не следует. У Евы уже утверждена native-архитектура: WinUI 3 — thin client, Rust Core — единственный владелец состояния и инструментов, supervisor — lifecycle и recovery, versioned protobuf over named pipe — единственный UI/Core transport.

## Что было изучено

В актуальном OpenHands/OpenHands просмотрены README, архитектурное описание Agent Canvas, дерево Agent Server и документация ACP/automations. Дополнительно проверены связанные репозитории OpenHands/software-agent-sdk и OpenHands/automation.

Зафиксированные наблюдения:

1. Agent Canvas отделяет отображение conversation, terminal, browser, files, settings и automation от выполнения действий агентом. Frontend не является sandbox и не должен напрямую выполнять действия.
2. Agent Server предоставляет CRUD разговоров и событий, WebSocket для realtime-потока, локальное хранение conversation/event/workspace, optional API-key auth, webhooks и redaction/encryption секретов.
3. Событийные схемы расширяемы: клиент должен переживать неизвестные варианты `kind`, а не считать перечисление закрытым.
4. Backend выбирается отдельно от Canvas; к одной оболочке можно подключить несколько Agent Server и переключаться между ними.
5. ACP запускает внешний CLI-агент как subprocess, общается с ним JSON-RPC по stdio и передаёт его события в оболочку. В документации приведены Claude Code, Codex и Gemini CLI.
6. Automations запускаются по cron или webhook, имеют историю запусков, enable/disable, профиль модели и отдельный backend исполнения.
7. OpenHands предупреждает, что локальный запуск без sandbox даёт агенту доступ к filesystem; для self-hosted режима нужны authentication, HTTPS, firewall и ограничение workspace.

## Целевое сопоставление с EvoHime

| Идея OpenHands | Реализация для Евы | Владелец |
| --- | --- | --- |
| Agent Canvas | Native WinUI workspace shell: chat, files, editor, Git, terminal, browser, automations | WinUI только отображает IPC-состояние |
| Agent Server | Внутренний evohime-core и его task/session API | Rust Core |
| Conversation/event store | SQLite WAL + append-only event log + replay по sequence ID | Rust Core |
| Backend registry | Реестр локального Core, удалённого Core и ACP backend | Core + IPC |
| ACP external agent | Отдельный `acp-runtime` с JSON-RPC stdio и Job Object | Core/supervisor |
| Automation Server | Локальный scheduler/trigger worker без отдельного обязательного сервера | Core + supervisor |
| Sandbox/workspace | Windows Job Object, allowlist workspace, policy profile, approval gate | Core + supervisor |
| Skills/plugins/MCP | Версионируемые capability packages с manifest, permission scope и health-check | Core |
| Terminal/browser/files | Существующие tool-runtime tools, доведённые до preview, streaming и resumability | Core |

## Приоритеты

### P0 — закончить единый цикл задачи и сделать его проверяемым

Это фундамент OpenHands-подобного опыта и одновременно следующий этап текущего EvoHime.

#### 1. Унифицированный event stream и task replay

Сделать типизированный внутренний event model вместо набора строковых событий:

- `TaskStarted`, `AssistantMessage`, `ToolCallRequested`, `ApprovalRequested`, `ToolOutput`, `FileChanged`, `DiffReady`, `TaskPaused`, `TaskResumed`, `TaskCompleted`, `TaskFailed`, `TaskCancelled`;
- у каждого события — `event_id`, `task_id`, `session_id`, `sequence_id`, timestamp, origin и redaction policy;
- неизвестные типы событий сохраняются и безопасно отображаются как `UnknownEvent`;
- replay восстанавливает timeline после перезапуска UI или reconnect;
- большие tool outputs хранятся как bounded chunks/artifacts, а не раздувают IPC frame.

IPC-изменения проводить только совместно в protobuf, Rust transport и C# envelope. Сохранять major/minor compatibility, bounded frame size, request IDs и sequence replay.

Критерии готовности: перезапуск WinUI не теряет поток; повторная доставка не создаёт дубликаты; старый клиент видит безопасный fallback; trace можно экспортировать в JSONL.

#### 2. Полноценный workspace cockpit

Завершить запланированные Files, Editor, Git и controlled Terminal как секции одной native-оболочки:

- tree файлов с фильтрами, file mentions и безопасным открытием;
- редактор только через Core-backed read/write/diff операции;
- Git status, diff, history, branch и commit preview;
- terminal с потоковым stdout/stderr, exit code, timeout и Stop;
- browser tool/session только через разрешённый Core runtime, с SSRF policy и явным preview навигации.

UI не получает прямой доступ к workspace и не запускает shell. Каждый изменяющий или потенциально опасный шаг сначала формирует preview, затем проходит approval.

Критерии готовности: пользователь видит, что именно изменится; отмена останавливает процесс и дочернее дерево; path traversal и выход за workspace невозможны; UI остаётся отзывчивым на длинном выводе.

#### 3. Capability-aware approval

Заменить общий approval на карточку, описывающую риск и границы действия:

- инструмент и конкретная операция;
- workspace-relative paths;
- команда с аргументами и предполагаемый cwd;
- сеть/домен, если действие сетевое;
- затронутые файлы и diff;
- прогнозируемые последствия, timeout и rollback strategy;
- одноразовое разрешение или scoped разрешение на сессию.

Политика должна проверяться повторно в Core непосредственно перед исполнением. WinUI может изменить решение пользователя, но не обходить policy.

## P1 — расширяемость и внешние агенты

### 4. Реестр backends и профили выполнения

Добавить Core-модель `BackendProfile`:

- `id`, display name, kind (`local`, `remote`, `acp`), endpoint/command;
- capability flags: files, shell, browser, Git, MCP, child agents, automation;
- auth reference без хранения секрета в SQLite;
- health status, version, last check и latency bucket;
- default model/agent profile и policy profile.

В UI добавить выбор backend в контексте проекта и задачи. Переключение не должно смешивать conversations или credentials. Для remote backend потребуется transport adapter, но native named pipe остаётся локальной границей UI/Core.

### 5. ACP bridge

Реализовать адаптер внешних CLI-агентов через JSON-RPC over stdio:

1. manifest команды, версии и capabilities;
2. запуск subprocess только из Core;
3. supervisor Job Object и graceful cancellation;
4. bounded stdout/stderr и нормализация событий ACP в общий event stream;
5. Credential Manager/DPAPI reference вместо передачи ключей в UI/logs;
6. явный approval перед запуском внешнего агента и перед передачей workspace;
7. compatibility tests на неизвестные ACP-сообщения и оборванный процесс.

Начать с одного внешнего backend и mock ACP server. Поддержку конкретных Claude/Codex/Gemini CLI считать профилями, а не бизнес-логикой WinUI.

### 6. Skills, plugins и MCP как единая capability-модель

У OpenHands расширения представлены разными поверхностями. Для Евы лучше объединить их в единый manifest:

- имя, версия, автор, совместимость Core;
- команды/tools и schema;
- разрешения: filesystem scopes, shell, network domains, secrets, child-agent;
- dependencies и conflict rules;
- health-check, disable/quarantine и audit trail;
- источник и SHA-256 пакета.

MCP transport должен быть отдельным адаптером tool-runtime, а не способом обойти approval. Установка и обновление расширения — через Core с проверкой подписи/хэша, backup и rollback.

## P2 — автоматизация и многосоставные задачи

### 7. Безопасный scheduler и triggers

Добавить локальные автоматизации как Core-owned сущности:

- cron/interval schedule;
- manual trigger;
- filesystem/Git event с debounce;
- webhook только при включённом localhost/auth boundary;
- enable/disable, next run, last run, status и run history;
- ограничение concurrent runs, timeout, retry/backoff и dead-letter state;
- выбранные backend, model profile, workspace и permission profile;
- обязательный dry-run/preview для первой настройки.

Первый полезный набор для Евы: проверка репозитория, анализ незакоммиченных изменений, запуск тестов по расписанию и подготовка отчёта. Автоматизация не должна сама делать commit/push или менять credentials без отдельной политики.

### 8. Дочерние исследовательские агенты

Сделать `ChildTask` для независимых read-only задач:

- parent/child IDs и отображение связи в timeline;
- отдельный context budget и deadline;
- read/search/Git status по умолчанию;
- запрещены write, shell mutation, commit, push и credential access;
- результат возвращается как структурированный artifact с источниками и confidence;
- родитель решает, включать ли результат в рабочий контекст.

После этого можно добавить scoped write-child, но только для изолированного snapshot/worktree и с отдельным approval.

### 9. Context budget и resumable tasks

Позднее объединить существующий план context/compact с OpenHands-подобной моделью длительной задачи:

- token/byte/event budgets с ранним предупреждением;
- summary checkpoint перед compact;
- полный trace сохраняется отдельно;
- pause/resume после перезапуска Core;
- handoff между backend профилями без потери задачи;
- artifact references вместо повторной передачи больших файлов и outputs.

## P3 — наблюдаемость и надёжность

### 10. Run history, diagnostics и privacy controls

Добавить экран истории запусков и Core API для:

- поиска задач по проекту, статусу, backend и времени;
- фильтрации tool events и approval decisions;
- открытия raw trace с явным предупреждением о чувствительных данных;
- экспорта redacted JSONL;
- локальных latency/failure/token/cost buckets;
- telemetry consent, `DO_NOT_TRACK` и полного выключения внешней отправки по умолчанию.

Разделить три уровня данных: продуктовые агрегаты, локальный диагностический trace и полный completion log. Секреты, prompt и содержимое файлов не отправлять наружу без отдельного явного режима.

### 11. Recovery и идемпотентность

Расширить supervisor/Core recovery:

- task lease и heartbeat;
- обнаружение зависшего child process;
- повторяемая доставка команд по request ID;
- idempotency key для write/commit/automation run;
- checkpoint перед долгим tool call;
- recovery UI с предложением resume, retry, rollback или discard;
- crash bundle без секретов.

## Что не переносить

- React, Vite, Zustand и Python Agent Server как runtime основы продукта;
- обязательный Docker для локального Windows-сценария;
- прямой REST/WebSocket между WinUI и Core;
- доступ UI к filesystem, SQLite, shell или model provider;
- неограниченный host filesystem access;
- автоматический commit/push без approval;
- telemetry по умолчанию;
- отдельную PostgreSQL-зависимость только ради автоматизаций;
- закрытые перечисления event kinds и schemas, которые ломаются при расширении.

## Предлагаемый порядок реализации

1. Event model, replay и безопасная нормализация неизвестных событий.
2. Files/Editor/Git/Terminal плюс diff и capability-aware approval.
3. BackendProfile и health/capability registry.
4. Skills/plugins/MCP manifest и permission scope.
5. ACP bridge с mock server и одним реальным профилем.
6. ChildTask read-only и artifact handoff.
7. Local scheduler, Git/filesystem triggers и run history.
8. Context budget, compact, pause/resume и recovery UI.
9. Privacy-aware diagnostics, export и финальный packaging/upgrade audit.

Каждый этап оформлять отдельным task-only commit в текущей ветке `main`. Для протокольных изменений обязательны Rust и C# compatibility tests. Для Rust — `cargo fmt --all -- --check` и targeted/full tests по затронутым crates; для WinUI — `dotnet test`; для пакета — native package smoke. Перед завершением — `git diff --check` и очистка generated artifacts.

## Критерии готовности плана

Ева должна:

- выполнять локальные и удалённые задачи через единый native cockpit;
- показывать живой, возобновляемый и проверяемый event timeline;
- подключать внешнего агента без передачи контроля WinUI;
- запускать повторяемые проверки по расписанию, но сохранять approval boundary;
- разделять родительские и read-only исследовательские задачи;
- расширяться через подписанные/проверяемые capabilities;
- восстанавливаться после падения без потери trace и без повторного опасного действия;
- не раскрывать секреты и не давать агенту доступ шире выбранной policy.

## Источники

- [OpenHands README и Agent Canvas](https://github.com/OpenHands/OpenHands) — product boundaries, local/self-hosted/cloud backends и automations.
- [Agent Canvas architecture](https://github.com/OpenHands/OpenHands/blob/main/docs/architecture.md) — границы frontend/runtime, backend registry и quality gates.
- [OpenHands Agent Server README](https://github.com/OpenHands/software-agent-sdk/tree/main/openhands-agent-server/openhands/agent_server) — REST/WebSocket, event persistence, auth, webhooks, redaction и extensible event schema.
- [ACP Agents](https://docs.openhands.dev/openhands/usage/agent-canvas/acp-agents) — JSON-RPC/stdio модель внешних CLI-агентов и credential boundary.
- [Pre-built Automations](https://docs.openhands.dev/openhands/usage/agent-canvas/prebuilt-automations) — cron/webhook сценарии, run management и профили моделей.
- [OpenHands Automation Service](https://github.com/OpenHands/automation) — scheduler, dispatcher, run history и retry-oriented service structure.
