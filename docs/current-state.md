# EvoHime — Current State

Last updated: 2026-07-22

## Stage: 7 active (Stages 1–6 foundations complete)

Normal tasks use native ReAct tool calling: the model selects a tool, receives its observation, and selects the next action until `assistant.reply`. Tool-level permission approvals remain enabled for protected operations.

Stages 1–6 foundations are complete. Stage 7 hardening/product items through `7.96` are implemented; scheduler correctness, request context, task timeline telemetry, log safety and deep health checks are hardened, and the next item is `7.97`.

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, workspace, GitHub, workers, memory, pipeline metrics + optional OTLP GenAI LLM spans |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema (incl. `memory.*`) |
| `memory` | Active | Redact/dedupe/conflict + retrieve + extract/gate + experience + feedback + hybrid embeddings (hash default, optional remote neural) |
| `storage` | Active | Sessions, tasks, events, messages, legacy notes, **memory_items**, settings, worker jobs |
| `tool-runtime` | Active | Sandboxed filesystem, shell, Git, browser, MCP call |
| `agent-runtime` | Active | Native ReAct loop; bounded iterations/calls; checkpoints; structured memory context |
| `model-gateway` | Active | Route-based gateway, LiteRouter + OpenAI-compatible + mock |
| `task-engine` | Active | Lifecycle, dependency batching, checkpoints, cancel/resume/retry |
| `permissions` | Active | ask/allow/deny + session/path overrides + temp allow + durable approval audit (PG) |
| `project-index` | Active | Chunk search, path/symbol weights, binary/noise filter (P2) |
| Python worker | Active | Health/stall reliability; handlers: stats/keywords/summarize/chunk/similarity/entities/diff/classify/language/redact |

## API endpoints

Errors return `{ "error", "code", "retryable" }` (plus `tool` / `approval_id` for approvals).

| Method | Path | Description |
| --- | --- | --- |
| GET | `/openapi.json` | Generated route-level OpenAPI contract |
| GET | `/health` | Health check (public) |
| GET | `/api/auth/status` | Local auth mode (`token_configured`, public) |
| GET | `/api/features` | Experimental feature flags and effective state |
| GET/PUT | `/api/models/config` | Model routes from web panel |
| GET | `/api/auth/github` | GitHub auth via local `gh` |
| GET | `/api/permissions` | Permission policy snapshot |
| GET | `/api/permissions/audit` | Durable approval audit (PG; `?limit=`) |
| GET | `/api/permissions/scopes` | Session overrides + path grants |
| PUT | `/api/permissions/:permission` | Update permission mode |
| GET | `/api/tools` | Tool catalog |
| GET | `/api/plugins` | Installed plugins under `.evohime/plugins` |
| GET | `/api/plugins/catalog` | Merged remote OSS marketplaces with semantic groups (`groups[]`, `category`/`group` on plugins); override via `EVOHIME_PLUGIN_CATALOG_URL` or `app_settings.plugin_catalog.sources` |
| POST | `/api/plugins/install` | Install catalog plugin via `git clone` into `.evohime/plugins` |
| GET/PUT | `/api/mcp/servers` | MCP server list |
| POST | `/api/sessions` | Create session + bootstrap |
| GET | `/api/sessions/:id/history` | Event history |
| GET | `/api/files` | List workspace entries |
| GET/PUT/POST | `/api/files/content` | Read / save / create file |
| GET | `/api/git/status` | Git status snapshot |
| GET | `/api/git/diff` | Diff (optional path) |
| POST | `/api/git/commit`, `/pull`, `/push` | Git mutations |
| GET | `/api/github/pull-requests` | PR list |
| GET | `/api/github/pull-requests/:number` | PR detail (diff, comments, reviews, checks) |
| POST | `/api/github/pull-requests` | Create PR via `gh` |
| GET | `/api/worker/status` | Worker metrics + DB status counts |
| GET | `/api/worker/jobs` | Recent durable worker jobs (`limit`, default 50) |
| POST | `/api/worker/jobs` | Submit worker job |
| GET | `/api/worker/jobs/:id` | Job state/result |
| POST | `/api/worker/jobs/:id/retry` | Retry job |
| GET | `/api/metrics` | Pipeline+worker snapshot + persist status |
| GET | `/api/metrics/history` | Persisted metrics snapshots from PG |
| GET | `/api/sites/:id/preview` | Workspace-scoped HTML site preview |
| POST | `/api/sites/:id/publish` | Workspace-scoped site publish |
| GET | `/metrics` | Prometheus text exposition |
| GET | `/api/memory` | List memory items (filters: scope, status, q, limit, cursor; keyset pagination) + privacy policy |
| GET | `/api/memory/export` | Export all portable memory items as JSON or ZIP (`format=json|zip`) |
| POST | `/api/memory/import` | Import JSON/ZIP memory pack through redaction, dedupe, conflict, and candidate admission |
| GET/PATCH/DELETE | `/api/memory/:id` | Get / update (content/status/pin, redacted) / delete |
| WS | `/ws/:session_id` | Real-time events |

## Agent flow

```text
user.message
  → save user message (session_messages)
  → load prior chat history
  → retrieve structured memory_items (budget + untrusted tag)
  → native tool call → observation → next action
  → bounded ReAct loop → respond
  → checkpoints / approval pause / resume
  → extract candidates on success only (LLM JSON + heuristic; skipped on task.failed)
  → admit → decision gate (auto-promote | memory.ask | drop)
  → legacy notes migrated at startup; structured memory_items only at runtime
  → task.completed | task.failed
```

## Database tables

- `sessions`, `tasks`, `task_steps`, `task_checkpoints`
- `session_events`, `session_messages`
- `session_memory` / `global_memory` — legacy free-text (migrate-only; imported at startup into `memory_items`)
- `memory_items` — structured scopes/items (`6.16`–`6.21`; runtime source of truth)
- `app_settings`, `worker_jobs` (incl. `claim_token` lease, `7.26`)
- `permission_approval_audit` — durable approval decisions (`7.23`)
- `metrics_snapshots` — periodic pipeline/worker snapshots (`7.24`)

## Frontend panels

| Panel | Status |
| --- | --- |
| Chat | ✅ |
| Settings | ✅ models, permissions, MCP, tools, worker, metrics, archive |
| Tasks | ✅ deep: steps, deps, pause reason, retries, recovery, approvals |
| Actions | ✅ deep: timeline + retry/approval/recovery metrics |
| Terminal / Files / Editor / Git | ✅ |
| Plugins | ✅ Installed list + remote catalog/install |
| Sites | ✅ Workspace-scoped preview and publish, feature-gated |
| Scheduled | ✅ Real scheduled jobs, feature-gated |
| Pull Requests | ✅ list + detail (diff/comments/checks) + create |
| Memory | ✅ tabs + playbook view + kind filters + JSON export; feedback; hybrid retrieve |

Frontend layout: `app.tsx` shell + `panels/` + typed `api/` + `hooks/useServerEventHandler` (`6.13` ✅). Brand: SVG mark + portrait mascot (`AgentBrand` / `AgentMark` / `AgentAvatar`, favicon).

## Tests

- `crates/agent-runtime` — pipeline integration: tool events → completion; approval pause → resume
- `crates/task-engine` — lifecycle integration: pause/resume/complete, fail/retry, recover_after_restart
- `crates/memory` — redact/normalize/dedupe/conflict + extract/gate + experience/playbooks + admit integration
- `crates/storage` — memory_items CRUD + overview/update/delete + legacy import
- `crates/model-gateway`, `protocol`, `tool-runtime`, `permissions`, `server`

## Next recommended step

`7.65` hardening выполнен: scheduler dispatch атомарно создаёт один run/session/task, manual trigger не увеличивает счётчик повторно, а failure history сохраняет ошибки. `7.93` добавил request-id, безопасные internal errors и header-only HTTP auth; `7.94` добавил copyable correlation ids и server-provided latency bars; `7.95` добавил tracing redaction и bounded health sampling; `7.96` добавил `/health/deep` с проверками DB, worker и workspace. Следующий пункт — `7.97`.

`7.56` выполнен: CI job `python-worker` на Python 3.12 запускает `python -m unittest discover -s workers/python -p "test_*.py"`; локально suite проходит: 25 тестов.

`7.69` выполнен: `useWebSocket` владеет reconnect/cursor/командами, `useChat` — chat/session state, `useWorkspace` — project/file/editor/Git state и операции. `app.tsx` уменьшен до 1968 строк.

`7.70` выполнен: `styles.css` сохранён как import-map, а исходный каскад разнесён на 8 самостоятельных CSS-файлов; объединённый текст совпадает с исходным SHA-256.

`7.71` выполнен: `PanelErrorBoundary` изолирует падение активной панели и Settings modal, показывает русский fallback с повторной попыткой и сбрасывается при переключении панели.

`7.87` выполнен: CI явно проверяет `cargo fmt --all -- --check`, а Rust docs job запускает `cargo doc` с `RUSTDOCFLAGS=-D warnings`, поэтому rustdoc warnings ломают сборку.

`7.88` выполнен: `.devcontainer/` поднимает переносимый workspace-контейнер, PostgreSQL 16 и Python worker через Compose; API/Web/worker ports и container env задокументированы в README.

`7.89` выполнен: `scripts/generate-openapi.mjs` извлекает 75 операций из `crates/server/src/routes.rs`, генерирует `docs/openapi.json` и `frontend/web/src/api/generated.ts`, а сервер отдаёт контракт через `/openapi.json`. Сейчас это route-level контракт; DTO-схемы остаются в domain API modules.

`7.90` выполнен: `/api/features` публикует Sites/Scheduled/OTLP flags; `EVOHIME_FEATURE_SITES` и `EVOHIME_FEATURE_SCHEDULED` скрывают панели и возвращают deep-link в чат, а `EVOHIME_FEATURE_OTLP=0` отключает экспорт OTLP.

Локальный `cargo fmt --all -- --check` сейчас обнаруживает pre-existing форматирование в нескольких файлах `crates/server` и `crates/storage`; эти посторонние изменения в `7.87` не включались.

`7.86` выполнен: CI job `protocol-drift` на Node 22 устанавливает frontend и корневые зависимости, запускает `npm run generate:protocol` и падает при diff в `frontend/web/src/protocol.generated.ts`.

`7.85` выполнен: CI job `frontend` — Node 22, `npm ci`, `npm run typecheck`, `npm run build` (Playwright smoke отложен).

`7.84` выполнен: CI job `test` поднимает `postgres:16`, задаёт `DATABASE_URL`/`EVOHIME_REQUIRE_DB`, а `connect_integration_pool` в CI падает вместо тихого skip.

`7.83` выполнен: sidebar больше не обрезает standalone-чаты до пяти — показываются все, sidebar скроллится. Wave D product polish закрыт.

`7.82` выполнен: удалены неиспользуемые `addModelRoute`/`removeModelRoute`, мёртвый placeholder fallback в `renderPanelContent`, CSS `.placeholderPanel`; EditorPanel переведён на `emptyPanelState`.

`7.81` выполнен: chip проекта показывает текущую git-ветку из `git status --short --branch` (не hardcoded `main`); парсинг через `parseGitBranchFromStatus`.

`7.80` выполнен: MemoryPanel и ActionsPanel приведены к русскому UI; EN action labels/details с WS переводятся через `translateActionLabel`/`translateActionDetail`; исправлен mojibake в «Загрузить ещё».

`7.79` выполнен: boot-загрузки permissions/audit/scopes/archive/github/projects/orchestrator больше не `.catch(() => undefined)` — ошибки собираются в `bootNotices`, показываются toast-баннером и дублируются в Settings.

`7.78` выполнен: `approval.granted` принимает `remember_path`; модал — «Один раз» vs «Запомнить путь (1 ч)»; audit показывает «путь запомнен».

`7.77` выполнен: `POST /api/plugins/uninstall`, `POST /api/plugins/update`, `GET /api/plugins/:name/skills`; PluginsPanel — удаление, обновление при расхождении версий, просмотр SKILL.md previews.

`7.76` выполнен: `POST /api/sessions/:session_id/unarchive` сбрасывает `archived_at`; в настройках → Архив кнопка «Восстановить» возвращает чат в sidebar и открывает его.

`7.75` выполнен: модал настроек вынесен в `SettingsModal` с `useModalA11y` (Escape закрывает, Tab не уходит наружу, фокус возвращается на триггер); вкладки — `tablist`/`tab`/`tabpanel` с arrow/home/end; ApprovalModal получил тот же trap и Escape=отказ.

`7.74` выполнен: у `ChatLine` стабильный `id` (React keys без remount при streaming), в шапке чата toggle «Показать/Скрыть ход» (localStorage), aria-labels на лог чата, composer и trace summary.

`7.73` выполнен: вложения теперь реально загружаются через `/api/sessions/:session_id/attachments`, пишутся в `.evohime/attachments/<session>/`, сохраняются в `session_attachments` и на старте задачи атомарно claim'ятся в attachment context для агента.

`7.72` + `7.68` выполнены: sidebar получил секцию «Инструменты» (Файлы, Редактор, Терминал, Git, Задачи, Действия); панели открываются через `?panel=` с поддержкой browser back/forward.

`7.65`–`7.67` выполнены: Scheduled tasks полностью реализованы.

`7.64` выполнен: Sites search/filter подключены к `GET /api/sites` с параметрами `q` и `status`; SitesPanel показывает вкладки «Все / Черновики / Опубликованные», debounced поиск и отдельные empty states.

`7.63` выполнен: Sites имеют workspace-scoped HTML preview, publish-операцию и открытие предпросмотра в браузере через реальный SitesPanel.

`7.36` реализован: `http.fetch` зарегистрирован с SSRF-защитой редиректов и лимитом текста; `filesystem.list` отражён в каталоге инструментов и матрице UI.

`7.37` реализован: registry теперь отменяет futures всех tools и параллельных вызовов, а shell дополнительно завершает дочерний процесс через `CancellationToken`.

`7.38` реализован: `OpenAICompatibleProvider` отделён от LiteRouter в factory, а env-конфигурация использует `OPENAI_API_KEY`, `OPENAI_BASE_URL` и `OPENAI_MODEL`; существующие `LITEROUTER_*` и сохранённые routes не меняются.

`7.39` реализован: composer получил route picker рядом с выбором модели, отправляет выбранный `model_route` и сохраняет route per-project в localStorage.

`7.42` реализован: admission memory использует kind-aware semantic dedupe по сохранённым embeddings после точного fingerprint; парафразы объединяются при cosine `>= 0.58`, unrelated и разные kinds не смешиваются.

`7.43` реализован: конфликтные записи показываются в MemoryPanel side-by-side, а выбор winner/loser проходит атомарно через `POST /api/memory/:id`.

`7.44` реализован: MemoryPanel получил ручное добавление memory item и шаблоны; backend прогоняет запись через redaction, normalization, embedding, dedupe и conflict flow.

`7.45` реализован: `/api/memory` поддерживает стабильный keyset cursor по sort key, а MemoryPanel дозагружает следующие страницы по 50 записей.

`7.46` реализован: удаление памяти требует подтверждения, а MemoryPanel показывает 8-секундное Undo с восстановлением через общий admission flow.

`7.47` реализован: `evohime-memory` поддерживает локальный ONNX provider через fastembed без API-ключа; модель кэшируется локально, а hash и remote режимы сохранены.

`7.48` реализован: retrieval выделяет до трёх релевантных структурированных playbook suggestions, а ReAct получает их как untrusted optional context без автоматического исполнения шагов.

`7.49` реализован: backend экспортирует переносимые memory items в JSON или ZIP с `memory.json`, а импорт прогоняет записи через redaction, normalization, embedding, dedupe и conflict admission как кандидатов.

`7.50` реализован как design milestone: спецификация multi-device sync определяет replica identity, append-only change log, cursor pull/push, snapshot recovery, offline outbox, tombstones и запрет тихого last-write-wins для конфликтов. Реализация разбита на `7.50a`–`7.50e`.

`7.51` реализован: Python worker и зеркальная Rust-валидация получили `text.classify`, `text.language` и `text.redact`; все три handler доступны также через `worker.run`.

**Актуализация 2026-07-22:** `7.51`, product honesty для Sites/Scheduled, scheduler correctness, request context, task timeline telemetry, log safety и deep health checks выполнены; Wave E `7.84`–`7.96` закрыта. `7.92` уже покрыт `7.24`; следующий пункт — `7.97`.

1. **Stage 7** — Waves A–D ✅; Wave E `7.84`–`7.96` ✅; next `7.97`
2. Рекомендуемая следующая волна: backup/export для sessions и memory
