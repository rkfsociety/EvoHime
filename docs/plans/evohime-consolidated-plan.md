# Единый план развития EvoHime

Дата сводки: 2026-08-11
Статус: консолидированный план следующей native-фазы
Область: Rust Core, SQLite, versioned named-pipe IPC, WinUI 3, supervisor

## 1. Назначение и результат

Этот документ объединяет 11 планов из `docs/plans` в один исполняемый roadmap. Цель — превратить EvoHime из native-чата с инструментами в локального диспетчера долгих, проверяемых и возобновляемых задач разработки и личных операций.

Итоговый пользовательский поток:

1. Пользователь формулирует цель или импортирует PRD/Markdown.
2. Core строит план и граф задач с зависимостями, оценками и критериями готовности.
3. Пользователь видит выбранные роль, skill, модельный маршрут, permissions и бюджет.
4. Core выполняет одну bounded-итерацию, сохраняет checkpoint и evidence.
5. Research, память, skills, tools и fallback подключаются только по policy.
6. На approval, failure, scope drift или превышении бюджета выполнение останавливается.
7. После перезапуска Core состояние восстанавливается через SQLite и IPC replay.

## 2. Границы, которые не меняются

- `EvoHime.exe` — WinUI 3/C# thin client; он отображает reducer-состояние и отправляет IPC-команды.
- `evohime-core.exe` — единственный владелец workspace, tools, permissions, approvals, orchestration, model routing, memory и SQLite.
- `evohime-supervisor.exe` — mutex, Job Object, lifecycle, restart/recovery, cleanup дочерних процессов и диагностика.
- Единственный транспорт — совместимый versioned named pipe `desktop-ipc-v1` с request IDs, sequence replay и bounded frame size.
- Секреты хранятся в Windows Credential Manager/DPAPI и не попадают в prompt, trace, package metadata или обычные логи.
- Web UI/Vite, прямой доступ UI к workspace/SQLite, перенос Python/Node runtime и произвольный внешний код не являются частью продукта.
- Автоматический shell, Git commit/push, сеть, установка расширений и внешние коннекторы всегда ограничены policy, budget, approval и audit.
- Работа ведётся в текущей ветке `main`; каждый этап заканчивается task-only commit, push — только по прямому запросу.

## 3. Что объединено

| Группа исходных планов | Сохранённые идеи |
| --- | --- |
| Task Master; Task Master + OpenJarvis | PRD → задачи, статусы, подзадачи, зависимости, `next_ready`, complexity analysis, research с citations, checkpoints, task workspace, exports, local-first routing, monitors |
| Mem0; Mem0 + LangGraph | append-only provenance, derived current view, области памяти, retrieval, temporal/entity signals, durable graph state, checkpoint/replay, связь памяти и графа без скрытой магии |
| Dify; LangChain | typed context, structured output, workflow graph, provider/model profiles, RAG, extension SDK, middleware, callbacks/traces, evaluation и native editor |
| OpenCode | явное разделение Plan/Build, управляемый context, постоянные сессии, diff/snapshots/rollback, subtask и compacting |
| OpenHands; Agent Reach | capability registry, backend fallback, Core Doctor, research pipeline, безопасный installer, scheduler/triggers, ACP bridge, operational profiles |
| Agency Agents; Agency Agents + skills | versioned Role/Skill contracts, lifecycle DEFINE → PLAN → BUILD → VERIFY → REVIEW → SHIP, deterministic discovery, handoff, deliverables, evals/hooks, read-only child roles, signed packs |

Дублирующиеся предложения не реализуются несколько раз: task graph, durable run state, skill registry, research, routing, memory и observability имеют по одному доменному контракту.

## 4. Целевая доменная модель

### 4.1. Задачи и граф

В SQLite добавить транзакционные сущности `projects`, `work_items`, `work_item_edges`, `work_item_events`, `work_item_tags`, `work_item_research` и `run_checkpoints`.

`work_item` хранит parent, title, description, source, priority, estimate, complexity, acceptance criteria, tag/workstream, status, attempt count и последний error. Статусы: `backlog`, `ready`, `in_progress`, `blocked`, `waiting_approval`, `done`, `cancelled`, `failed`.

Граф должен атомарно проверять отсутствующие ссылки, запрещать циклы, учитывать зависимости и priority при выборе `next_ready`, а переходы статуса — сохранять события и быть идемпотентными.

### 4.2. Запуск и workflow

Каждый run имеет `task_id`, `run_id`, `checkpoint`, lifecycle stage, budget, policy, tool calls, diff, evidence, approval state и stop reason. Состояния lifecycle: `defined`, `planned`, `building`, `verifying`, `reviewing`, `ready_to_ship`, `blocked`, `completed`.

Workflow graph поддерживает typed inputs/outputs, условия, retries, timeout, cancellation, human approval, subgraph и bounded loop. Автоматический loop выполняет только одну ограниченную итерацию за раз и останавливается при failure, scope change, неожиданном diff, неоднозначном acceptance criteria или budget limit.

### 4.3. Роли и skills

```text
SkillDefinition {
  id, version, title, description, triggers, lifecycle_stage,
  required_context, references, allowed_tools, risk_class,
  approval_policy, steps, deliverables, acceptance_criteria,
  eval_suite, hooks, author, source, integrity
}

RoleDefinition {
  id, division, identity, mission, communication_style,
  skill_ids, default_model_route, read_only, delegation_policy
}
```

Начальный native-каталог: onboarding, product spec, planner, native Windows engineer, Rust test engineer, WinUI UX reviewer, code reviewer, security/privacy auditor, release/packaging engineer и minimal-change engineer. Personality влияет только на объяснение результата и не даёт полномочий.

### 4.4. Память и research

Память разделяется на profile/preferences, project facts, decisions, task history и ephemeral run context с TTL. Первичным источником остаётся append-only event/provenance, а текущие факты — производным представлением с version и confidence.

Research сохраняет source kind, URL/path, title, fetched-at, hash, redacted excerpt, citations, freshness и связь с work item. Раздельно учитываются web, локальные документы и workspace. Непроверенный текст исследования не становится автоматически trusted prompt-контекстом.

### 4.5. Capability и provider

Capability registry описывает tools, skills, MCP, модели, каналы, triggers и external agents через manifest с checksum/source/version, permissions, allowed domains и input/output schema. Provider profiles: `local-first`, `balanced`, `cloud-research`, `offline`. Для каждого запроса учитываются capability, context size, privacy class, latency/price budget и доступность.

## 5. Единый порядок поставки

### Этап 0 — контракты, storage и recovery foundation (P0)

Зависимости: существующие IPC/SQLite foundations.

- Создать миграции task graph, run checkpoints, memory provenance и базовых workflow entities.
- Определить protobuf-команды для task CRUD, dependency validation, checkpoint/resume, lifecycle и graph/status/progress events.
- Добавить idempotency, replay после рестарта, backup перед миграцией, rollback и атомарные переходы.
- Реализовать durable run state, cancellation token, timeout, bounded output и supervisor recovery.
- Заложить typed envelope/context и unknown-field compatibility fixtures.

Выход: после убийства/перезапуска Core граф, статус и последний подтверждённый checkpoint восстанавливаются без двойного запуска.

Проверки: Rust unit/integration tests миграций, cycles, races двух runners, replay, cancellation, rollback; C# compatibility tests; `cargo fmt --all -- --check`.

### Этап 1 — Plan/Task Core и task workspace (P0)

- Добавить безопасный импорт PRD/Markdown с сохранением исходного текста, версии и происхождения каждой задачи.
- Реализовать ручное создание, decomposition, complexity analysis, dependency graph и `next_ready`.
- В WinUI показать Projects/Tasks: ready, blocked, done, граф, карточку, подзадачи, acceptance criteria и event history.
- Добавить действия Следующая задача, Разблокировать, Отложить, Запустить, Остановить, Повторить и Отметить готовой; Core подтверждает каждый переход.
- Не менять файлы только из-за импорта PRD.

Проверки: parser diagnostics, malformed PRD, duplicate import, cycle/missing dependency, concurrent update, UI IPC smoke и truthful blocked/ready states.

### Этап 2 — Plan/Build lifecycle, context и snapshots (P0)

- Разделить `/spec`, `/plan`, `/build`, `/test`, `/review`, `/ship`; Plan read-only, Build — только утверждённый scope.
- Добавить context assembler: task, acceptance criteria, выбранные references, memory, research и budget; workspace не попадает целиком скрыто.
- Реализовать сессии, compaction, snapshot/diff, preview изменений, rollback и восстановление после ошибки.
- Ввести human approval перед записью, опасным tool, scope change и delivery.

Проверки: plan не пишет, build не выходит за scope, prompt/context budget ограничен, snapshot rollback восстанавливает состояние, reconnect не теряет lifecycle.

### Этап 3 — Research и typed workflow graph (P0)

- Реализовать workflow nodes с typed input/output, condition, retry, timeout, cancellation, approval и subgraph.
- Реализовать research pipeline: запрос → разрешённый HTTP/search → извлечение → краткое резюме → citations → сохранение.
- Ограничить domains, время, размер ответа и стоимость; добавить refresh и cancel.
- Для security, dependency и API-вопросов поддержать policy, требующую research перед запуском.

Проверки: citation/source integrity, stale results, prompt-injection fixtures, network deny, retry/backoff, partial failure и deterministic workflow replay.

### Этап 4 — Skills, roles и capability registry (P0/P1)

- Добавить registry/parser/validator определений Role/Skill и deterministic matcher по intent, файлам, языкам, lifecycle stage и project rules.
- UI показывает выбранные role/skill, version, причины, risk, tools и acceptance criteria; пользователь может закрепить или заменить выбор.
- Ввести lifecycle snapshot: активная definition immutable в рамках run.
- Поддержать skill/capability manifest, effective permissions, allowed domains и разделение инструкций, MCP и исполняемых расширений.
- Установка — только локальный архив или HTTPS с manifest, hash/signature и совместимостью; install scripts запрещены по умолчанию; update staged, rollback сохраняется.

Проверки: invalid risk/tool, missing reference, version conflict, hash/signature mismatch, path escape, prompt injection, unknown skill и disable/rollback.

### Этап 5 — Безопасный task loop и model routing (P1)

- Runner: выбрать `next_ready`, собрать task/research/skill context, выполнить bounded run, записать checkpoint и предложить следующий шаг.
- `run_policy`: max iterations, wall-clock timeout, tool-call/token budget, network policy, approval mode и stop conditions.
- Автоматически остановиться на approval, failure, unexpected diff, budget, scope drift или неясном критерии.
- Добавить local-first/balanced/cloud-research/offline routing и явный visible fallback.
- Логировать redacted provider/model, latency, tokens, retries, estimated cost и причину маршрутизации.

Проверки: offline execution, provider unavailable, fallback policy, token/tool budgets, stop/resume/pause, supervisor Job Object cleanup и отсутствие silent cloud route.

### Этап 6 — Memory v1 и RAG для локального workspace (P1)

- Добавить memory domain/API: create, list, search, update, archive, forget и provenance inspection.
- Интегрировать extraction фактов и решений после run только по policy; пользователь подтверждает важные записи.
- Реализовать scoped retrieval project/task/workspace, lexical/vector hybrid search, recency, confidence, entity/temporal signals и ссылки на первичное событие.
- Добавить compression старых traces, TTL для ephemeral context, privacy labels, export/delete и redaction.

Проверки: scope isolation, stale/conflicting facts, delete/forget, migration rollback, no secret leakage, retrieval relevance fixtures и offline operation.

### Этап 7 — Evals, hooks, observability и Core Doctor (P1)

- Evals для skill selection, allowlist, plan quality, IPC compatibility, cancellation, replay, citations, memory retrieval, routing и UI truthfulness.
- Hooks `before_context`, `before_tool`, `after_tool`, `before_commit`, `after_task` только наблюдают или отклоняют по policy и не получают секреты.
- Локальный JSONL/SQLite audit trail: versions, tool calls, approvals, durations, failures, budgets, diffs и evidence.
- Core Doctor проверяет pipe, storage/migrations, providers, permissions, tools, scheduler и recovery; UI показывает actionable diagnostics.
- Feedback: useful/not useful, correction, rejection reason, successful/failed tool result; наружная telemetry только opt-in.

Проверки: bounded trace, redaction, no secret leakage, deterministic eval fixtures, doctor failure simulation и восстановление после повреждённого checkpoint.

### Этап 8 — Child roles, handoff и native workflow editor (P1/P2)

- Разрешить дочерние read-only задачи для onboarding, code search, threat-model review, test-plan review и документации.
- Child получает урезанный context и `child_task_id`, не имеет write, shell, commit, install или network mutation tools.
- Родитель проверяет структурированный report, confidence и sources перед включением в plan/build.
- В WinUI добавить catalog, workflow editor, timeline child tasks/evals/hooks/evidence, approval state и понятные blocked/error states.
- Поддержать handoff contract с inputs, outputs, acceptance criteria, risks и evidence.

Проверки: child write/shell/commit denial, timeout/cancel, bounded output, parent-child visibility, editor round-trip и visual smoke.

### Этап 9 — Schedules, proactive Pulse и внешние каналы (P2)

- Сделать schedule/trigger/monitor state/last checkpoint/next run/retry/backoff/dead-letter entities.
- Начать с локальных источников: GitHub notifications, workspace changes, CI status, local files и task deadlines.
- Supervisor запускает monitor с теми же budgets, permissions, approvals и cancellation, что и обычный run.
- Pulse показывает digest, новые события, пропущенные запуски и degradation; failure не скрывается уведомлением.
- После стабилизации локального контура добавить ACP/external-agent gateway и отдельные коннекторы.

Проверки: missed run, duplicate trigger, dead letter, backoff, restart, cancellation, permission denial и отсутствие внешней мутации без approval.

## 6. Native UI-поставка

Последовательность экранов: reducer/state → shared theme и three-zone frame → Projects/Tasks → task detail/graph → composer Plan/Build → role/skill catalog → research/memory inspector → run timeline/doctor → workflow editor → Pulse/schedules.

Визуальное направление сохраняется: graphite/dark surfaces, violet/turquoise accents, compact desktop density, оригинальная iconography. UI обязан честно различать Empty, Loading, Ready, Running, Degraded, Error и Blocked, а не показывать обещанные действия как доступные.

WinUI не хранит state, не читает SQLite/workspace, не запускает installer и не принимает решение о permissions. Все данные, команды и ошибки приходят через IPC.

## 7. Безопасность и отказоустойчивость

- Allowlist и `risk_class` вычисляет Core; prompt, Role и Skill не могут расширить права.
- Каждая опасная операция получает approval, timeout, cancellation, bounded output и redacted audit record.
- Path traversal, archive escape, изменённый manifest, неподписанный package и недопустимый domain отклоняются до выполнения.
- Child processes принадлежат supervisor Job Object; restart/recovery не создаёт дублей.
- Migration всегда транзакционная и предваряется backup; corrupted state переводит систему в диагностируемый blocked state.
- Research, memory, logs и traces очищают секреты, токены, полный чувствительный context и prompt injection payloads.
- Commit/push, публикация и внешние connector actions остаются отдельными явно разрешёнными действиями.

## 8. Общий quality gate каждого этапа

Перед завершением этапа должны пройти свежие проверки затронутых компонентов:

- `cargo fmt --all -- --check`;
- targeted Rust unit/integration/compatibility tests;
- WinUI/C# IPC и UI tests;
- native workflow/package smoke при затрагивании packaging/runtime;
- `git diff --check`;
- проверка запуска staging EXE, IPC reconnect/replay и truthful UI при UI-изменениях;
- очистка ненужных `target/`, `bin/`, `obj/` и временных package artifacts.

Каждый этап оформляется отдельным task-only commit. Нельзя считать плановую функцию реализованной до появления теста и evidence в trace.

## 9. Что сознательно не переносится

- Полные каталоги внешних personas/divisions, marketing-агенты и personality-driven permissions.
- Чужие Node/Python CLI, обязательные MCP-серверы, installers, shell scripts, UI, названия и точная runtime-структура.
- Произвольный код из Markdown skills, install scripts и capability, обходящие policy.
- Автоматические ветки, бесконечные loops, silent cloud fallback, auto-commit/push и внешний research без approval/audit.
- Graph database как обязательный storage layer; сначала SQLite и provenance.
- Внешние календари и почта до стабилизации локальных schedules/monitor protocol.
- Возврат web UI/Vite или бизнес-логики в WinUI.

## 10. Итоговые критерии готовности инициативы

Инициатива готова, когда одновременно выполнены следующие условия:

1. Из требования получается draft-plan с non-goals, зависимостями, acceptance criteria, complexity и понятным `next_ready`.
2. Plan/Build/Verify/Review/Ship разделены, а каждое автоматическое действие имеет policy, budget, timeout, cancellation и approval.
3. Core переживает restart во время research, tool call, workflow и loop, восстанавливая checkpoint через SQLite и IPC replay.
4. Пользователь видит текущие task, role, skill, model route, permissions, diff, trace, evidence и причину blocked/error.
5. Skills, packages, child roles, research и memory проходят manifest/permission/redaction checks и могут быть отключены или откачены.
6. Local/offline route работает без облака, а fallback видим и разрешён policy.
7. Rust, C#, native package, security, eval, UI smoke и `git diff --check` проходят свежие проверки.
8. Нет скрытого доступа UI к workspace/SQLite, секретов в trace и непреднамеренной внешней мутации.

## 11. Порядок после этой сводки

Сначала реализовать Этап 0 и Этап 1 как фундамент: без устойчивого task graph, checkpoint/replay и ручного task workspace остальные идеи останутся временными обходами. Затем последовательно добавить lifecycle/context, research/workflow, skills, bounded loop/routing, memory, evals, child roles и только после стабилизации — schedules и внешние каналы.