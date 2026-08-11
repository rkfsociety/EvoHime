# План развития Евы по мотивам OpenHands и Agent Reach

Дата: 2026-08-11  
Статус: предложение к реализации  
Основание: изучение [OpenHands](https://github.com/OpenHands/OpenHands) и [Agent Reach](https://github.com/Panniantong/agent-reach)

## 1. Краткий вывод

Оба проекта предлагают полезные не для копирования интерфейсы, а архитектурные идеи:

- OpenHands Agent Canvas объединяет несколько agent backends, допускает локальные, удалённые и облачные среды, запускает автоматизации по расписанию или webhook-событию и умеет работать с ACP-совместимыми агентами.
- Agent Reach является capability layer: он подбирает, устанавливает, проверяет и маршрутизирует инструменты доступа к источникам данных. Для каждого канала задаётся упорядоченный список основного и резервного backend, а `doctor` показывает фактическое состояние и рецепт исправления.

Для EvoHime это следует реализовать как native-возможности Rust Core, не возвращая web UI, HTTP-сервер или прямой доступ WinUI к workspace. UI только отображает состояние и отправляет команды через существующий versioned named-pipe IPC.

## 2. Что добавить Еве

### 2.1. Capability Registry — реестр возможностей и каналов

Добавить в Core реестр возможностей, где каждый capability описывает:

- стабильное имя и версию;
- назначение и допустимые операции: чтение, поиск, запись, публикация;
- backend-кандидаты в порядке приоритета;
- требования: программа, MCP-сервер, авторизация, сеть;
- уровень риска, лимиты, таймаут и необходимость approval;
- диагностический probe и понятный remediation hint.

Первый набор: `web.read`, `web.search`, `github.read`, `github.issues`, `rss.read`, `youtube.transcript`, `filesystem.search`, `git.inspect`. Социальные сети и публикация не входят в первый MVP.

Предлагаемые Rust-модули:

- `crates/capability-registry/` — модели, manifest и выбор backend;
- расширение `crates/tool-runtime/` — выполнение capability через уже существующий sandbox, SSRF и risk gate;
- `crates/evohime-local-storage/` — состояние установленных/проверенных backend и история health-check.

### 2.2. Надёжная маршрутизация «основной + резервный backend»

Перенести в общий runtime паттерн Agent Reach: конкретная интеграция не зашивается в agent loop, а выбирается по ordered list. Backend считается доступным только после реального probe, а не после проверки наличия бинарника.

Требования:

1. определить capability;
2. проверить кандидатов с bounded timeout;
3. выбрать первый полностью рабочий;
4. при временной ошибке перейти к резервному и записать причину;
5. вернуть в timeline фактически выбранный backend;
6. не повторять опасную write/publish-операцию автоматически без approval.

Это даст замену внешнего инструмента без переписывания core и сделает деградацию честной: Ева сообщает, что именно недоступно и почему.

### 2.3. Core Doctor и страница диагностики

Добавить команду Core `RunCapabilityDoctor` и native-раздел «Диагностика».

Doctor должен показывать:

- capability;
- статус `Ready`, `NeedsSetup`, `Unavailable`, `Degraded`;
- выбранный backend и резерв;
- последнюю проверку, latency и версию;
- отсутствие зависимости, проблемы сети, авторизации или sandbox;
- безопасную инструкцию исправления;
- кнопку «Проверить снова».

По умолчанию doctor только читает состояние. Установка или изменение системы выполняется отдельной командой после явного approval, с preview/dry-run и журналом результата.

### 2.4. Локальный research pipeline

Собрать первую безопасную цепочку интернет-исследования:

`search -> fetch -> normalize -> extract -> cite -> summarize`.

Минимальные возможности:

- чтение HTML через отдельный reader с SSRF-защитой;
- поиск с явно указанным provider и лимитом запросов;
- RSS/Atom;
- публичные GitHub README, issues и pull requests через `gh` или официальный API;
- извлечение субтитров YouTube, если доступно легальным способом.

Каждый результат должен сохранять URL, источник, время получения, тип данных, hash содержимого и предупреждение о неполноте. В ответе Ева обязана отделять извлечённые факты от своего вывода.

Не включать в MVP автоматический сбор cookies, обход блокировок, скрытую браузерную авторизацию и автоматическую публикацию. Для приватных источников использовать только явно переданные пользователем учётные данные через Credential Manager/DPAPI.

### 2.5. Безопасный capability installer

Сделать управляемый установщик возможностей по образцу Agent Reach:

- `inspect` — только проверка;
- `plan` — полный dry-run с перечнем изменений;
- `install` — только после approval;
- `doctor` — реальный probe;
- `remove` — удаление capability и его конфигурации с отдельным подтверждением.

Core должен исполнять установку в контролируемом окружении, ограничивать дочерние процессы Job Object/supervisor и не принимать команды установки из непроверенного текста или внешней веб-страницы без показа пользователю.

Секреты не записывать в SQLite, события, prompts или JSONL. В SQLite хранить только ссылку на credential id, scope и время последней проверки.

### 2.6. Автоматизации и расписания

Взять у OpenHands идею Automation Server, но реализовать её как локальный native scheduler внутри Core:

- ручной запуск;
- расписание с часовым поясом;
- событие из локального filesystem/Git;
- webhook только в отдельном opt-in режиме и с локальной авторизацией;
- сохранение task template, capability allowlist и лимитов;
- повтор с backoff, отмена и дедупликация;
- журнал каждого запуска и итоговый отчёт.

Supervisor должен переживать перезапуск Core, а SQLite — хранить состояние очереди идемпотентно. Любые внешние действия (`git push`, публикация, изменение системы) проходят approval policy.

### 2.7. ACP/внешний agent gateway

Исследовать ACP как совместимый boundary для подключения внешнего агента, не отдавая ему прямой доступ к данным EvoHime.

В первой версии поддержать только:

- запуск внешнего процесса в отдельном Job Object;
- ограниченный набор capability;
- поток событий и cancellation;
- request id, sequence replay и bounded frame size;
- явное отображение внешнего backend в UI.

Нельзя разрешать внешнему агенту обходить permissions, approval, sandbox, redaction или владельца SQLite. При несовместимости протокола использовать адаптер, а не менять `desktop-ipc-v1` без compatibility tests.

### 2.8. Развитие native UI

Добавить в существующую оболочку следующие экраны:

1. «Возможности» — включённые каналы, backend и разрешения;
2. «Диагностика» — Core Doctor;
3. «Источники» — результаты исследования с цитатами и состоянием свежести;
4. «Автоматизации» — шаблоны, расписание, история запусков;
5. «Подключения» — provider/credential status без отображения секретов;
6. «Внешние агенты» — backend, capability allowlist и lifecycle.

Все экраны получают reducer state из IPC. Состояния `Ready`, `NeedsSetup`, `Running`, `Degraded`, `Error` должны быть различимы и подтверждаться событиями Core.

## 3. Приоритет реализации

### Фаза A — фундамент

- capability manifest и реестр;
- ordered backend selection;
- события диагностики и IPC compatibility tests;
- Core Doctor без установки;
- audit trail и redaction tests.

### Фаза B — полезный research MVP

- `web.read`, `web.search`, `rss.read`, `github.read`, `youtube.transcript`;
- нормализация источников, цитаты и hash;
- лимиты, cancellation, SSRF и тестовые fake backends;
- native экран «Возможности» и timeline tool output.

### Фаза C — управление окружением

- dry-run/install/remove;
- Credential Manager/DPAPI bindings;
- approval preview для изменения системы;
- recovery после прерванной установки;
- doctor remediation hints.

### Фаза D — автоматизации

- SQLite schema для templates, schedules, runs и leases;
- Core scheduler и supervisor recovery;
- локальные filesystem/Git triggers;
- history, retry, deduplication и уведомления.

### Фаза E — ACP и расширенные каналы

- ограниченный ACP adapter;
- внешние agent backends;
- приватные источники только через explicit credential flow;
- дополнительные каналы после security/legal review и реальных probes.

## 4. Контракт и структура данных

Добавить совместимые IPC-команды и события, не ломая major-версию:

- команды: `ListCapabilities`, `RunCapabilityDoctor`, `InstallCapabilityPlan`, `ConfirmCapabilityInstall`, `RemoveCapability`, `CreateAutomation`, `RunAutomation`, `CancelAutomation`, `ListSources`;
- события: `capability.status`, `capability.backend_selected`, `capability.diagnostic`, `research.source`, `research.citation`, `automation.started`, `automation.retrying`, `automation.completed`, `external_agent.status`.

Для каждой операции обязательны `request_id`, `task_id` при наличии, `sequence_id`, elapsed time, redacted error code и origin (`core`, `backend`, `external-agent`). Сырые cookies, access tokens и authorization headers запрещены в payload и логах.

## 5. Тестовая стратегия и критерии готовности

- unit tests для manifest validation, ordered fallback, probe timeout, redaction и policy decisions;
- integration tests с fake backends: основной недоступен, резерв успешен, оба недоступны;
- IPC compatibility tests для новых команд/событий и replay после перезапуска Core;
- security tests для SSRF, path escape, secret leakage, approval bypass и child-process cleanup;
- SQLite migration/backup/recovery tests для scheduler;
- WinUI tests для doctor, деградации, source citations и automation history;
- native workflow smoke с временным Git-репозиторием и полностью локальными fake providers;
- перед завершением: `cargo fmt --all -- --check`, целевые `cargo test`, `dotnet test desktop/EvoHime.Tests/EvoHime.Tests.csproj -p:Platform=x64`, `git diff --check`, cleanup `target/bin/obj` и проверка task-only diff.

Критерий MVP: Ева может безопасно найти и прочитать публичный источник, показать цитату и фактический backend, пережить недоступность основного backend через резервный, объяснить проблему через Doctor и не раскрыть секреты.

## 6. Что не переносить из изученных проектов

- веб-панель и Electron/React как пользовательский runtime;
- прямой запуск агентного сервера с полным доступом к filesystem;
- неограниченный shell/install из prompt;
- скрытое использование browser cookies;
- автоматические обходы блокировок и массовый scraping;
- cloud/remote backend без отдельной модели доверия, consent и audit;
- бизнес-логику в WinUI.

## 7. Источники и ограничения исследования

- [OpenHands README](https://github.com/OpenHands/OpenHands) описывает Agent Canvas, Agent Server, несколько backend, automations и ACP.
- [Agent Reach README](https://github.com/Panniantong/agent-reach) описывает capability layer, список каналов, ordered fallback, `doctor`, dry-run/install и локальное хранение credential material.

Описанные в этом документе backend и доступность внешних сервисов считаются изменяемыми. Перед реализацией каждого канала нужен свежий probe, проверка лицензии/условий использования и отдельный security review.
