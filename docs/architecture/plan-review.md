# EvoHime — доменный контракт: ревью планов, правка и архитектурный дрейф

Канонический контракт домена ревью и правки планов, Plan Artifact и Architecture Drift Review. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).

Ревью планов — отдельный read-only pipeline Core. Electron main выбирает и
ограниченно читает Markdown-файл через native dialog, затем передаёт его Core.
Core вызывает 2–8 моделей текущего provider catalog по очереди, по одному
запросу за раз, чтобы не упираться в лимиты провайдера, и затем отдельную
synthesis-модель. Per-request model overrides сохраняются. Состав и порядок
рецензентов фиксируются на момент запуска: неудачное обновление каталога
возвращает пустой список и трактуется как «нет новостей», а не как «нет
моделей», поэтому уже выбранные модели не теряются. Исходный Markdown
ограничен 512 КБ, ответ каждого рецензента — 256 КБ. На ревью можно подать
несколько файлов сразу (диалогом или перетаскиванием): оболочка склеивает их в
один документ с нумерованными разделами и проверяет суммарный размер. Перед
запуском объём запроса сверяется с окном каждой выбранной модели: заведомо не
влезающий план блокирует запуск, а худший случай синтеза (план плюс все ответы
рецензентов) остаётся предупреждением. Review не получает tools,
не изменяет workspace и сохраняется в локальном event journal без credentials;
история ревью очищается отдельной командой и исчезает из UI сразу, а не после
перезапуска.

Правка плана по ревью — второй шаг того же pipeline и такой же read-only на
стороне модели: один вызов synthesis-модели получает исходный план и текст
ревью и возвращает переписанный план целиком. Диффа не запрашивается — модели
надёжнее воспроизводят документ, чем адресуют куски. Текст ревью Core берёт из
своего кэша или журнала, а не из запроса оболочки, поэтому выдать за ревью
произвольный текст нельзя. Результат живёт в памяти Core до отдельной команды
сохранения, показывается пользователю целиком и записывается только по его
решению — поверх оригинала или в новый файл; расширение `.md` проверяет Core.
Правка работает по одному файлу: склеенный из нескольких планов документ
нельзя однозначно разложить обратно.

## Plan Artifact v1 (план 57)

`evohime-local-storage::plan_artifact` — единственный mutable authority для
versioned planning contract. SQLite schema 56 хранит append-only revisions и
execution snapshots; canonical camelCase JSON без `content_hash` хешируется
SHA-256. Contract limits: 128 steps, 64 criteria, 32 references, 4 KiB text
fields и 64 KiB canonical artifact. Состояния проходят только явные переходы
`draft → accepted → executing → paused|replan_required|completed|failed|unknown_outcome`;
re-plan создаёт новую revision. `ExecutePlan` фиксирует exact revision/hash и
policy snapshot, но не добавляет shell/network authority: реальный effect
остаётся за существующим Core task/workflow boundary и повторной policy,
capability и approval проверкой. После restart неизвестный dispatch не
повторяется вслепую. Legacy `TaskPlanSpec`, plan context и plan review — только
read-only inputs.

Plan Artifact остаётся внутренним Core/agent/model контрактом. Core владеет его
валидацией, версионированием и переходами состояний; модель может только
предложить ограниченное содержимое через агентский runtime. Electron не
предоставляет ручную вкладку, идентификаторный lookup или renderer-команды
create/read/transition/execute. Наружу публикуются только bounded
task/progress projections, необходимые обычному пользователю. Raw prompts,
model output, secrets, absolute paths и executable identities в Plan Artifact
boundary запрещены.

## Incremental Change Protocol v1 (план 59)

`evohime-core::incremental_change_protocol` связывает bounded
`RequirementDelta` и `ImpactAnalysis` с exact revision/hash существующих Plan
Artifact и Workspace State Checkpoint. SQLite schema 58 хранит только
versioned run metadata, fingerprints, redacted evidence и idempotency key;
исходные prompts/output, absolute paths и capability grants не сохраняются.
До перехода в `applied` Core сверяет optimistic version и observed scope
fingerprint; drift даёт `stale` без записи. `cancelled` и
`unknown_reconciliation_required` terminal и не ретраятся автоматически.

Authenticated IPC command 210/event 57 остаётся внутренним Core-контрактом для
оркестратора: модель не может сама придумать ссылки на plan/checkpoint и
обойти их проверку. Electron не показывает ручную панель и не принимает
bounded JSON от пользователя; Core использует протокол только после получения
типизированных ссылок на Plan Artifact и Workspace State Checkpoint. Базовый
executor не выполняет внешний effect; будущие adapters обязаны добавить
отдельный policy/approval contract.

## Architecture Snapshot & Drift Review v1

Core владеет bounded metadata-only снимком компонентов, связей, границ,
evidence и coverage. Extraction читает только allowlist манифестов, IPC proto и
архитектурной документации, не исполняет файлы и не передаёт renderer исходный
текст. Evidence содержит revision и SHA-256; candidate/unsupported/stale не
повышаются до verified автоматически. Снимки хранятся в SQLite schema 89,
refresh не удаляет последнюю принятую запись при ошибке. Delta сравнивает все
архитектурные поверхности, review требует baseline hash, а неизвестные
IPC-операции отклоняются fail-closed (command 258/event 103).
