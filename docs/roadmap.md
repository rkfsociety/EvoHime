# EvoHime — Дорожная карта

> Обновлено: 2026-07-17

## Обзор

```text
Этап 1 ✅ → 2 ✅ → 3 ✅ → 4 ✅ → 5 ✅ → 6 ✅ → Этап 7 🟡
Фундамент   Модель   Tools   Editor   Tasks   Advanced   Hardening + Product
```

| Этап | Название | Статус | Ключевой результат |
| --- | --- | --- | --- |
| 1 | Фундамент | ✅ Готово | Vertical slice работает |
| 2 | Чат с моделью | ✅ Готово | LiteRouter streaming |
| 3 | Tools + shell | ✅ Готово | Sandbox, filesystem tools, shell, approvals, terminal, permissions |
| 4 | Editor + Git | ✅ Done | Browser file tree, Monaco editor, Git status/diff/actions, and synchronization events |
| 5 | Оркестрация | ✅ Complete | Lifecycle, команды, storage и recovery готовы |
| 6 | Advanced | ✅ Foundations done | Memory, PR, workers, observability, tool catalog |
| 7 | Hardening + Product | 🟡 Plan | Security, reliability, Sites/Scheduled, agent 2.0, CI, DX |

---

## Milestone 0 — Vertical slice ✅

**Цель:** доказать, что весь pipeline работает.

```text
Сообщение → задача → stream → filesystem.read → UI → PostgreSQL
```

### Чеклист

- [x] `POST /api/sessions` — создание сессии
- [x] `WS /ws/:session_id` — real-time события
- [x] `user.message` — команда клиента
- [x] `task.started` / `task.completed` / `task.failed`
- [x] `agent.message.delta` — потоковый ответ
- [x] `agent.plan.updated` — план агента
- [x] `tool.started` / `tool.output` / `tool.completed`
- [x] `filesystem.read` с защитой path traversal
- [x] История в `session_events` (PostgreSQL)
- [x] Frontend: чат + timeline событий
- [x] Native local launcher: PostgreSQL + server + web
- [x] JSON Schema → TypeScript codegen

### Критерий готовности

Пользователь отправляет сообщение в браузере, видит потоковый ответ, результат `filesystem.read`, и после перезагрузки история доступна через API.

---

## Milestone 1 — Этап 2: Чат с моделью ✅

**Зависимости:** Milestone 0 ✅

**Цель:** заменить демо-агент на реальную LLM.

### Deliverables

| # | Задача | Статус |
| --- | --- | --- |
| 2.1 | Model gateway: абстракция провайдера | ✅ |
| 2.2 | LiteRouter адаптер (OpenAI-compatible) | ✅ |
| 2.3 | Streaming токенов → `agent.message.delta` | ✅ |
| 2.4 | Конфигурация модели через env / API | ✅ |
| 2.5 | История сообщений в БД | ✅ |
| 2.6 | `agent_loop.rs` вместо demo vertical slice | ✅ |
| 2.7 | UI: панель Settings | ✅ |
| 2.8 | Тесты model-gateway + agent-runtime | ✅ |

### Критерий готовности

Агент отвечает через выбранную LLM с потоковой передачей. История сообщений сохраняется и восстанавливается при переподключении.

---

## Milestone 2 — Этап 3: Tools, shell, permissions

**Зависимости:** Milestone 1

**Цель:** агент работает с файловой системой, shell и запрашивает разрешения.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 3.1 | `filesystem.write` | ✅ `crates/tool-runtime/` |
| 3.2 | `filesystem.patch` | ✅ `crates/tool-runtime/` |
| 3.3 | `filesystem.search` (ripgrep) | ✅ `crates/tool-runtime/` |
| 3.4 | `shell.execute` с песочницей | ✅ `crates/tool-runtime/` |
| 3.5 | Permission engine: check + request | ✅ `crates/permissions/` |
| 3.6 | `approval.required` event + UI modal | ✅ `protocol`, `server`, `frontend/web/` |
| 3.7 | `approval.granted` / `approval.denied` commands | ✅ `protocol`, `server` |
| 3.8 | xterm.js терминал (панель Terminal) | ✅ `frontend/web/` |
| 3.9 | Таймауты и отмена инструментов | ✅ `crates/tool-runtime/` |
| 3.10 | UI: настройки разрешений | ✅ `frontend/web/` |
| 3.11 | Тесты: sandbox, permissions, каждый tool | ✅ `crates/tool-runtime/`, `crates/permissions/`, `crates/server/` |

### Критерий готовности

Агент читает/пишет файлы, выполняет shell-команды. Опасные действия требуют подтверждения в UI. Терминал показывает вывод.

---

## Milestone 3 — Этап 4: Editor, files, Git ✅

**Зависимости:** Milestone 2

**Цель:** полноценная работа с кодом через браузер.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 4.1 | API: список файлов / содержимое / сохранение | ✅ `crates/server/` |
| 4.2 | Дерево файлов (панель Files) | ✅ `frontend/web/` |
| 4.3 | Monaco Editor (панель Editor) | ✅ `frontend/web/` |
| 4.4 | `git.status`, `git.diff` tools | ✅ `crates/tool-runtime/` |
| 4.5 | `git.commit`, `git.pull`, `git.push` tools | ✅ `crates/tool-runtime/` |
| 4.6 | Git diff viewer (панель Git) | ✅ `frontend/web/` |
| 4.7 | `file.changed` event | ✅ protocol, server |
| 4.8 | `git.diff.changed` event | ✅ protocol, server |
| 4.9 | Тесты git tools | `crates/tool-runtime/` |

### Критерий готовности

Пользователь навигирует по файлам, редактирует в Monaco, видит Git status/diff, агент может коммитить через инструменты.

---

## Milestone 4 — Этап 5: Task orchestration ✅

**Зависимости:** Milestone 3

**Цель:** надёжный оркестратор задач с recovery.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 5.1 | Реальное планирование (LLM → plan steps) | ✅ `crates/agent-runtime/` |
| 5.2 | Параллельное выполнение независимых tools | ✅ `crates/tool-runtime/`, `crates/task-engine/` |
| 5.3 | `task.cancel` command | ✅ protocol, server |
| 5.4 | Отмена running tools | ✅ `crates/tool-runtime/`, `crates/server/` |
| 5.5 | `task.resume` — продолжение с checkpoint | ✅ `crates/task-engine/`, `crates/server/` |
| 5.6 | Повторный запуск failed tasks | ✅ `crates/task-engine/` |
| 5.7 | Recovery после restart сервера | ✅ `crates/task-engine/`, `crates/storage/`, `crates/server/` |
| 5.8 | Панель Tasks — список и статусы | ✅ `frontend/web/` |
| 5.9 | Панель Actions — журнал действий | ✅ `frontend/web/` |
| 5.10 | Тесты: cancel, resume, recovery | ✅ `crates/task-engine/`, `crates/agent-runtime/`, `crates/server/` |

### Критерий готовности

Задачи можно остановить, продолжить и перезапустить. После рестарта сервера running tasks восстанавливаются. Независимые tools выполняются параллельно, а план шагов сохраняется в `task_steps` и отображается в history/events.

**Статус:** Milestone 4 завершён.

---

## Milestone 5 — Этап 6: Advanced

**Зависимости:** Milestone 4

**Цель:** production-ready агент с расширяемой экосистемой.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 6.1 | Project index (embedding / ripgrep) | ✅ `crates/project-index/` |
| 6.2 | Контекстный поиск для агента | ✅ `crates/agent-runtime/` |
| 6.3 | `mcp.call` tool | ✅ `crates/tool-runtime/` |
| 6.4 | MCP server management UI | ✅ `frontend/web/`, `crates/server/` |
| 6.5 | Agent memory (persistent context) | ✅ `crates/storage/`, `crates/agent-runtime/` |
| 6.6 | Multi-model routing: task-scoped routes + OpenAI-compatible providers | ✅ `crates/model-gateway/`, `frontend/web/`, `crates/server/` |
| 6.7 | Python workers (HTTP/queue) | ✅ reliability + summarize/chunk/similarity/entities/diff; more optional | `workers/python/` |
| 6.8 | `browser.open`, `browser.extract` | ✅ `crates/tool-runtime/` |
| 6.9 | Settings: models, permissions, MCP, tools | ✅ `frontend/web/`, `crates/server/` |
| 6.10 | Тесты: index, MCP, workers | ✅ respective crates / `workers/python/` |

### 6.16–6.25 — Память, опыт и ask-on-uncertainty самообучение

**Цель:** превратить текущие `session_memory` и `global_memory` из журнала коротких заметок в структурированную автоматическую систему памяти для **локального single-tenant** использования. EvoHime не изменяет веса модели: самообучение = накопление фактов, опыта, ошибок и playbooks. По умолчанию агент решает сам; при сомнении или high-impact — спрашивает. Панель Memory — прозрачность и override, не обязательный approve на каждую запись.

Спека: [2026-07-16-agent-memory-design.md](superpowers/specs/2026-07-16-agent-memory-design.md)

| # | Задача | Статус | Crate / Path |
| --- | --- | --- | --- |
| 6.16 | Memory model: session, workspace, project, global(=user), experience | ✅ | `migrations/0013_memory_items.sql`, `crates/storage/src/memory.rs` |
| 6.17 | Структурированные memory items: kind, confidence, importance, source, status, supersedes, pinned | ✅ schema | `migrations/0013_memory_items.sql`, `crates/storage/src/memory.rs` |
| 6.18 | Memory service: нормализация, redaction секретов, дедупликация и разрешение конфликтов | ✅ | `crates/memory/` |
| 6.19 | Memory retrieval: scope filtering, ranking, budget, untrusted tagging + `memory.search` | ✅ | `crates/memory/src/retrieve.rs`, `agent-runtime`, `server` |
| 6.20 | Memory extraction + decision gate: auto-promote или ask-on-uncertainty | ✅ | `crates/memory/` extract+gate, `server`, protocol, MemoryAskModal |
| 6.21 | Experience memory: success/failure patterns, verification rules и playbooks | ✅ | `crates/memory/` experience+extract, retrieval priority, gate |
| 6.22 | Override UI: правка, отклонение, архив, удаление, pin (не блокер happy path) | ✅ | `/api/memory`, MemoryPanel (+ playbook view / export polish) |
| 6.23 | Feedback loop: memory used/helpful/corrected/rejected и confidence decay | ✅ | `crates/memory/src/feedback*.rs`, migration `0016` |
| 6.24 | Панель Memory: active, candidates, experiences, conflicts и privacy | ✅ | `frontend/web/src/panels/MemoryPanel.tsx` |
| 6.25 | Hybrid semantic retrieval через embeddings после стабилизации lexical retrieval | ✅ hash + optional remote neural (`EVOHIME_EMBEDDING_MODE=remote`) | `crates/memory/src/embed.rs`, migration `0017` |

#### Архитектурные правила памяти

- Single-tenant: один оператор на машине; `user` = alias для `global`.
- Системные правила и явный текущий запрос пользователя имеют приоритет над памятью.
- Память в prompt — **untrusted data**, не system instructions.
- Project/workspace memory не смешивается с памятью другого workspace (identity: path + git remote id).
- По умолчанию extract → candidate → auto-promote при высокой уверенности; ask только при uncertainty / conflict / high-impact / global-pin/constraint.
- `candidate` влияет слабо и не является законом; `conflict` не попадает в prompt.
- Секреты, токены, пароли, cookies и private keys запрещено сохранять в память.
- Каждая запись имеет источник, confidence и возможность удаления/override.
- Prompt budget: pinned → active → experience → weak candidate.
- Embeddings добавляются только после проверки качества структурированной памяти и lexical retrieval.

#### Критерии готовности memory milestone

- Память разделена на session, workspace, project, global и experience.
- Happy path полностью автоматический; ask срабатывает только при сомнении/высоком импакте.
- Retrieval выбирает релевантные записи, ограничивает бюджет и пишет `used_memory_ids`.
- Новые записи проходят redaction, валидацию, дедупликацию и проверку конфликтов.
- Оператор может изменить, отклонить, pin и удалить любую запись без обязательного approve на каждый extract.
- Агент использует прошлые успешные и неудачные решения как опыт.
- Смена workspace не приводит к утечке памяти между проектами.
- Полный memory flow покрыт storage, integration и security-тестами.

### Критерий готовности

Агент использует project index для контекста, вызывает MCP-инструменты, работает с несколькими моделями. Python workers принимают структурированные HTTP jobs с process/job heartbeat, typed payloads и прикладными хендлерами (`text.summarize`, `text.chunk`); observability task pipeline (`GET /api/metrics`) landed — следующий шаг интеграционные тесты и оркестрация/UI к масштабированию.

---

## Следующий слой улучшений

После закрытия базового Stage 6 приоритет смещается с "фича есть" на "система масштабируется, восстанавливается и остаётся удобной в сопровождении".

### P1 — Архитектура и оркестрация

| Приоритет | Улучшение | Что добавить |
| --- | --- | --- |
| P1 | Реальный executor плана | ✅ `plan → execute(batches) → observe → replan → respond` (до 3 replan) |
| P1 | Усиленный checkpoint/recovery | ✅ plan + pause_reason + approval_wait; merge; resume skips completed steps |
| P1 | Наблюдаемость task pipeline | ✅ correlation id + logs + `GET /api/metrics` + optional OTLP + Settings Metrics UI |
| P1 | Больше интеграционных сценариев | ✅ `pipeline_integration` + `lifecycle_integration` (approval pause/resume, recovery) |

### P1 — UI и сопровождение фронтенда

| Приоритет | Улучшение | Что добавить |
| --- | --- | --- |
| P1 | Декомпозиция `frontend/web/src/app.tsx` | ✅ types + api + lib + panels + event hook (`6.13`) |
| P1 | Typed API client | ✅ `frontend/web/src/api/*` поверх `apiRequest` |
| P1 | Более глубокие панели Tasks/Actions | ✅ steps/deps/pause/retries/approvals/recovery |
| P1 | Agent memory 6.16–6.25 | ✅ `6.16`–`6.25` done |

### P2 — Поиск, разрешения и GitHub workflow

| Приоритет | Улучшение | Что добавить |
| --- | --- | --- |
| P2 | Улучшенный project index | ✅ chunks + path/symbol weights + binary/noise filter (`crates/project-index/`) |
| P2 | Более гибкая permission model | ✅ per-session / per-path overrides, temp allow on grant, approval audit (`crates/permissions/`) |
| P2 | Расширение GitHub/PR панели | ✅ diff/comments/reviews/checks/create PR (`6.14`) |

### P2 — Workers и фоновые процессы

| Приоритет | Улучшение | Что добавить |
| --- | --- | --- |
| P2 | Укрепление worker subsystem | ✅ health/stall + API + Settings Worker UI (status/jobs/retry) |
| P2 | Специализированные ML handlers | ✅ `text.summarize` / `chunk` / `similarity` / `entities` / `diff` (+ stats/keywords); further optional |

### Кандидаты на следующий milestone

- `6.11` План-исполнитель с реальным графом шагов и повторным планированием ✅ (batches + bounded replan)
- `6.12` Расширенные checkpoints, approvals recovery и task replay ✅
- `6.13` Декомпозиция frontend shell (`app.tsx` -> panels/hooks/services) ✅
- `6.14` GitHub PR workflow: diff, review comments, checks, create PR ✅
- `6.15` Worker reliability: retries, heartbeat, stalled-job handling ✅
- `6.16`–`6.18` Memory schema + service ✅
- `6.19` Memory retrieval into agent loop ✅
- `6.20` Memory extraction + ask-on-uncertainty ✅
- `6.21` Experience memory / playbooks ✅
- `6.22` / `6.24` Memory panel overrides ✅
- `6.23` feedback loop ✅
- `6.25` hybrid embeddings ✅
- Pipeline observability (`GET /api/metrics`) ✅
- Integration tests (approval pause/resume, recovery) ✅
- General LLM tool-calling across registered tools ✅ (`browser.*`, `mcp.call` wired; planner catalog)
- P2 improved project index ✅ (chunks, path/symbol weights, binary filter)
- P2 finer permissions ✅ (session/path overrides, temp allow, audit)
- Optional OpenTelemetry OTLP export ✅ (`OTEL_EXPORTER_OTLP_ENDPOINT`)
- Optional remote neural embeddings ✅ (`EVOHIME_EMBEDDING_MODE=remote`)
- More ML handlers ✅ (`text.similarity`, `text.entities`)
- Memory UI / experience polish ✅ (playbook view, kind filters, export)
- Deeper worker observability ✅ (`/api/worker/status`, job list, metrics)
- On-demand `memory.search` tool ✅ (registry + agent-runtime DB path)
- Frontend worker dashboard ✅ (Settings → Worker)
- Frontend pipeline metrics dashboard ✅ (Settings → Metrics)
- Worker `text.diff` handler ✅ (stdlib difflib + Rust validation)
- **Next:** Stage 7 — Hardening + Product (см. ниже)

---

## Этап 7 — Hardening, Product Surface & Scale

**Цель:** превратить «работает локально у одного оператора» в продукт, который безопасно слушает сеть, переживает рестарты, честно показывает UI и масштабирует агента/память/workers. Пункты собраны по аудиту кода (2026-07-17); размеры: **S** ~1–2 дня, **M** ~3–7 дней, **L** ~1–3 недели.

**Принцип приоритизации:** security → reliability → agent correctness → product stubs → DX/CI → moonshots.

### 7.A — Security & trust boundary

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.1 | Локальный auth на HTTP + WebSocket (token / bind-loopback + optional bearer) | L | ✅ | `EVOHIME_API_TOKEN`, middleware, `/api/auth/status`, UI Settings |
| 7.2 | Жёсткий CORS allowlist вместо `CorsLayer::permissive()` | S | ✅ | `EVOHIME_CORS_ORIGINS` / `EVOHIME_CORS_PERMISSIVE`; defaults Vite+local |
| 7.3 | Default `BIND_ADDR=127.0.0.1` (+ docs / launcher) | S | ✅ | `app.rs`, `start-dev.ps1`, `.env.example`, AGENTS |
| 7.4 | SSRF guard для `browser.*` (block localhost / private / link-local / metadata) | M | ✅ | `ssrf.rs` + browser validate/redirect/final; `EVOHIME_SSRF_ALLOW_PRIVATE` |
| 7.5 | SSRF guard для `mcp.call` + optional allowlist hosts | M | ✅ | `ssrf` + redirect/final; `EVOHIME_MCP_ALLOWED_HOSTS` |
| 7.6 | Shell: scrub / allowlist env (не наследовать API keys) | M | ✅ | `shell_env.rs`; allowlist + secret scrub; `EVOHIME_SHELL_*` |
| 7.7 | Encrypt-at-rest для API keys в `app_settings` (или OS keychain) | M | ⬜ | model config в PG plaintext |
| 7.8 | Plugin install: pin commit/tag, signature/hash, uninstall/update | L | ⬜ | `server/src/plugins.rs` |
| 7.9 | Plugin skills quarantine (не все skills → system prompt без opt-in) | L | ⬜ | `agent_loop` workspace rules |
| 7.10 | Permission для `memory.search` + audit | S | ⬜ | сейчас `PERMISSIONS: &[]` |
| 7.11 | Rate limiting / concurrency caps на sessions, tasks, worker jobs | M | ✅ | `rate_limit.rs`; 429 + WS `rate.limited`; `EVOHIME_RATE_LIMIT_*` |
| 7.12 | Git push/pull network policy (remote allowlist, deny force) | M | ⬜ | `tools/git.rs` |
| 7.13 | Content-Security-Policy / secure headers для static web | S | ⬜ | Vite/static serve path |
| 7.14 | Secrets scan в CI (gitleaks / similar) | S | ⬜ | `.github/workflows` |

### 7.B — Reliability & recovery

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.15 | Поднять Python worker из `start-dev.ps1` (+ tray icon) | M | ✅ | `-Worker`, tray, health wait `:8090`, auto-restart |
| 7.16 | LLM provider retry / backoff / retry-after | M | ✅ | `retry.rs` + literouter initial-request retries; `EVOHIME_LLM_*` |
| 7.17 | WebSocket reconnect + event resume (cursor / last event id) | M | ✅ | `HistoryItem` WS envelope; `after_sequence` / history `?after=`; frontend auto-reconnect |
| 7.18 | Safe restart policy: не auto-resume mutating tasks без флага | M | ✅ | recover RETURNING only; mutating defer; `EVOHIME_AUTO_RESUME_ON_RESTART` |
| 7.19 | PgPool tuning (max_connections, timeouts, idle) | S | ✅ | `storage/pool.rs`; `EVOHIME_PG_*` env knobs |
| 7.20 | Observability locks без `.expect()` panic | S | ✅ | `observability` / `worker_observability`: poison → `into_inner` |
| 7.21 | `filesystem.search` graceful fallback без `rg` | S | ✅ | ripgrep preferred; walk fallback + `engine` field; `EVOHIME_SEARCH_FORCE_FALLBACK` |
| 7.22 | Persist permission session/path grants across restart | M | ✅ | `permission_scopes` setting; export/import; save on approval grant |
| 7.23 | Durable permission approval audit (PG table) | M | ✅ | `permission_approval_audit`; sink from engine; GET reads PG |
| 7.24 | Persist / scrape pipeline+worker metrics (or Prometheus scrape) | M | ✅ | PG `metrics_snapshots` + `/metrics` Prometheus; `/api/metrics/history` |
| 7.25 | Task-engine: `transition` по id, cancel через FSM | S | ✅ | `load_task` + FSM; cancel/retry validated |
| 7.26 | Worker: уменьшить dual-state races (lease / claim token) | M | ✅ | `claim_token` CAS; steal on recovery; stale complete ignored |
| 7.27 | Structured error taxonomy в API (`code`, `retryable`) | M | ✅ | `api_error.rs`; client parses `code`/`retryable` |

### 7.C — Agent runtime & tools 2.0

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.28 | Native provider `tool_calls` (OpenAI-compatible tools array) | L | ✅ | `chat_with_tools` + planning path; text fallback; `EVOHIME_NATIVE_TOOL_CALLS` |
| 7.29 | Распилить `agent_loop.rs` (plan / execute / context / parse) | L | ✅ | `agent_loop/{mod,parse,plan,execute,context,util}.rs` |
| 7.30 | Распилить `server/main.rs` на routers/modules | L | ✅ | `startup`/`routes`/`task/*`/`*_api`/`ws`; main ~64 LOC |
| 7.31 | Multi-agent / subagent fan-out с бюджетом | L | ✅ | tool `agent.run` + depth/concurrency/step/timeout budgets |
| 7.32 | Streaming tool progress (partial output events) | M | ✅ | `tool.output.delta` + shell stdout/stderr stream |
| 7.33 | Tool result truncation + summarization budget | M | ✅ | `tool_budget.rs`; head/tail + total chars env caps |
| 7.34 | Planner cost/latency telemetry per step | M | ✅ | GenAI OTLP spans + usage tokens + `/api/metrics` llm_* |
| 7.35 | `assistant.reply` + user-visible plan edits (approve plan) | M | ✅ | PostgreSQL checkpoint, edited dependencies, WebSocket approve/reject, durable paused state |
| 7.36 | More tools: `filesystem.list` в матрице UI; `http.fetch` с SSRF policy | M | ✅ | list surfaced in tool catalog/matrix; fetch registered with redirect and final-url SSRF checks |
| 7.37 | Cancel mid-tool with cooperative cancellation everywhere | M | ✅ | registry dispatcher cancels every tool future; shell receives token for child-process termination |
| 7.38 | Separate OpenAICompatible provider from LiteRouter alias | S | ⬜ | `model-gateway` |
| 7.39 | Model route picker в composer (не только Settings) | M | ⬜ | frontend chat |

### 7.D — Memory 2.0

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.40 | Выключить dual-write legacy `session_memory`/`global_memory` (migrate-only) | M | ✅ | post-task writes only `memory_items`; prompt via retrieve |
| 7.41 | Startup/one-shot `import_legacy_memory_notes` wired | S | ✅ | server startup; idempotent `source_label` markers |
| 7.42 | Semantic / fuzzy dedupe (не только fingerprint) | M | ⬜ | `memory/dedupe.rs` |
| 7.43 | Conflict UI: side-by-side resolve / supersede flow | M | ⬜ | MemoryPanel conflicts tab |
| 7.44 | Manual «добавить память» + templates | M | ⬜ | API list/patch/delete only |
| 7.45 | Pagination / cursor для `/api/memory` (лимит 150) | M | ⬜ | MemoryPanel |
| 7.46 | Memory delete confirm + undo window | S | ⬜ | frontend |
| 7.47 | Local embedding model option (onnx / candle) без remote API | L | ⬜ | hash default + remote only |
| 7.48 | Experience playbook auto-suggest in planner | M | ⬜ | retrieve есть; planner не специализирован |
| 7.49 | Memory export/import workspace pack (zip/json) | M | ⬜ | JSON export partial |
| 7.50 | Multi-device sync (out of earlier memory scope) | L | ⬜ | design out-of-scope follow-up |

### 7.E — Workers & ML

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.51 | Ещё handlers: `text.classify`, `text.language`, `text.redact` | M | ⬜ | further optional |
| 7.52 | Agent tool `worker.run` (submit+await job) | M | ✅ | tool-runtime HTTP submit+poll; wiremock test |
| 7.53 | Worker job UI: submit form + payload editor в Settings | M | ⬜ | сейчас status/list/retry |
| 7.54 | Horizontal worker scale (N processes / queue backend) | L | ⬜ | single in-proc Python queue |
| 7.55 | Typed JSON Schema registry для worker tasks (shared) | M | ⬜ | duplicate validate Rust/Python |
| 7.56 | CI job для `workers/python` unittest | S | ⬜ | не в workflow |

### 7.F — Project index & context

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.57 | Persistent on-disk index + incremental invalidate | L | ⬜ | full walk each search |
| 7.58 | Symbol / AST-aware chunks (tree-sitter optional) | L | ⬜ | path/symbol weights уже есть |
| 7.59 | Embeddings for project chunks (reuse memory embed pipeline) | L | ⬜ | comment «future encoder» |
| 7.60 | Sidebar global search (сейчас кнопка без handler) | M | ⬜ | `app.tsx` |
| 7.61 | `@file` / `@symbol` mentions в composer | M | ⬜ | attachments сейчас имена-only |

### 7.G — Product UI: Sites & Scheduled

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.62 | Sites: data model + `/api/sites` CRUD | L | ⬜ | `SitesPanel` stub |
| 7.63 | Sites: preview / publish / open-in-browser | L | ⬜ | product surface |
| 7.64 | Sites: search/filter wired to real data | S | ⬜ | `siteSearch` dead |
| 7.65 | Scheduled: real cron/timer jobs (storage + runner) | L | ⬜ | `ScheduledPanel` templates only |
| 7.66 | Scheduled: honest copy (убрать fake mail/calendar claims) | S | ⬜ | misleading recommendations |
| 7.67 | Scheduled: list/pause/delete active schedules | M | ⬜ | после 7.65 |
| 7.68 | Deep-link / router для панелей (`?panel=`, history) | M | ⬜ | нет router |

### 7.H — Frontend shell, UX, a11y

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.69 | Дораспилить `app.tsx` → hooks (`useWebSocket`, `useChat`, `useWorkspace`) | L | ⬜ | ~1800 LOC, ~80 useState |
| 7.70 | CSS modules / split `styles.css` | L | ⬜ | ~3000+ lines |
| 7.71 | React Error Boundary per panel | M | ⬜ | `main.tsx` |
| 7.72 | Орphan panels: Files/Git/Terminal/Tasks/Actions в навигации | M | ⬜ | panels есть, entry points слабые |
| 7.73 | Реальные file attachments (upload + server store + tool access) | M | ⬜ | сейчас только имена в тексте |
| 7.74 | Chat: stable message keys, tool lines toggle, a11y labels | S | ⬜ | `app.tsx` |
| 7.75 | Settings modal: Escape, focus trap, tabpanel pattern | M | ⬜ | Settings + Approval tabs |
| 7.76 | Chat archive restore / unarchive | M | ⬜ | delete-only archive |
| 7.77 | Plugins: uninstall, update, skill browser | M | ⬜ | install-only |
| 7.78 | Approval modal: remember-path / temp-allow controls | M | ⬜ | audit знает `remembered_path` |
| 7.79 | Silent boot errors → toast / Settings banner | M | ⬜ | `.catch(() => undefined)` |
| 7.80 | i18n consistency (RU/EN mix в Memory/Actions) | S | ⬜ | MemoryPanel actions |
| 7.81 | Project chip: real git branch (не hardcoded `main`) | S | ⬜ | `app.tsx` |
| 7.82 | Dead code cleanup (`addModelRoute` unused, placeholderPanel) | S | ⬜ | frontend |
| 7.83 | Show all chats (не `slice(0,5)` без «ещё») | S | ⬜ | sidebar |

### 7.I — Protocol, CI, DX

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.84 | CI: PostgreSQL service + storage/memory integration tests | L | ⬜ | tests skip без `DATABASE_URL` |
| 7.85 | CI: frontend `tsc` + build (+ optional playwright smoke) | M | ⬜ | только `rust.yml` |
| 7.86 | CI: protocol schema ↔ Rust ↔ generated TS drift check | M | ⬜ | manual sync today |
| 7.87 | CI: Clippy `-D warnings` already; add fmt/docs gates docs for stage 7 | S | ⬜ | keep current |
| 7.88 | Devcontainer / cross-platform launcher (не только Windows tray) | L | ⬜ | `start-dev.ps1` WinForms |
| 7.89 | OpenAPI / typed HTTP client gen из server routes | L | ⬜ | сейчас hand-written `api/*` |
| 7.90 | Feature flags (`EVOHIME_FEATURE_*`) для experimental surfaces | M | ⬜ | Sites/Scheduled/OTLP |
| 7.91 | Docs sync: `development-plan.md` / `AGENTS.md` / `current-state` под Stage 7 | S | ⬜ | этот PR |

### 7.J — Observability & ops

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.92 | Prometheus `/metrics` exposition (или OTEL metrics) | M | ✅ | done in 7.24 (`GET /metrics`); OTLP traces already optional |
| 7.93 | Per-request/request-id on HTTP (не только task correlation) | S | ⬜ | |
| 7.94 | Task timeline UI: correlation id copy + latency bars | M | ⬜ | Actions/Tasks panels |
| 7.95 | Log sampling / redaction of secrets in tracing | M | ⬜ | shell/env, model keys |
| 7.96 | Health endpoint: deep checks (DB, worker, disk) | S | ⬜ | сейчас `{status:ok}` |
| 7.97 | Backup/export: sessions+memory dump CLI | M | ⬜ | |

### 7.K — Moonshots / Stage 8 candidates

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.98 | Team / multi-operator mode (authz scopes) | L | ⬜ | single-tenant today |
| 7.99 | Cloud sync / remote workspace | L | ⬜ | |
| 7.100 | Visual browser agent loop (CDP session reuse) | L | ⬜ | browser tools one-shot |
| 7.101 | Eval harness (golden tasks, regression agents) | L | ⬜ | |
| 7.102 | Marketplace for playbooks / plugins with trust scores | L | ⬜ | memory design out-of-scope |
| 7.103 | Online continual learning (still no weight fine-tune; stronger experience) | L | ⬜ | |
| 7.104 | Mobile-responsive shell (browser-only, no native app) | M | ⬜ | web-first rule |
| 7.105 | Voice input / TTS optional | M | ⬜ | |
| 7.106 | Diff review UI for agent patches before apply | L | ⬜ | approvals coarse today |
| 7.107 | Worktree-aware multi-checkout agent (parallel tasks isolated) | L | ⬜ | |
| 7.108 | Cost budgets & spend caps per day/model | M | ⬜ | LiteRouter free/paid |
| 7.109 | Self-update channel for launcher | M | ⬜ | |
| 7.110 | Formal threat model doc + abuse cases | M | ⬜ | |

### Suggested Stage 7 delivery waves

**Актуальный статус 2026-07-18:** `7.37` ✅ — отмена распространяется на обычный и параллельный запуск всех tools; следующий пункт Wave C — `7.38`.

1. **Wave A (trust):** `7.1`–`7.6`, `7.11`, `7.15`–`7.16` ✅ → Wave B next  
2. **Wave B (survive restarts):** `7.17`–`7.27`, `7.40`–`7.41` ✅ → Wave C next  
3. **Wave C (agent quality):** `7.28`–`7.37`, `7.52` ✅ → next `7.38`+
4. **Wave D (product honesty):** 7.62–7.67, 7.72–7.73, 7.66  
5. **Wave E (DX/CI):** 7.84–7.86, 7.56, 7.69–7.71  
6. **Wave F (scale/moonshots):** 7.54, 7.57–7.59, 7.98+

### Критерий готовности Stage 7 (минимум)

- Локальный сервер по умолчанию не торчит в LAN без auth  
- SSRF blocked для browser/MCP  
- Launcher поднимает Python worker  
- WS reconnect не теряет критичные события  
- Legacy memory dual-write выключен или явно deprecated  
- Sites/Scheduled либо реализованы, либо убраны из «как будто работают»  
- CI гоняет frontend + Postgres integration + Python worker tests  

---

## Матрица: панели UI × этапы

| Панель | Этап | Milestone | Статус |
| --- | --- | --- | --- |
| Chat | 1 | M0 | ✅ Активна |
| Events (timeline) | 1 | M0 | ✅ Активна |
| Files | 4 | M3 | ✅ Complete |
| Editor | 4 | M3 | ✅ Complete |
| Terminal | 3 | M2 | ✅ Active |
| Git | 4 | M3 | ✅ Complete |
| Tasks | 5 | M4 | ✅ Deep: steps, deps, pause, retries, recovery |
| Actions | 5 | M4 | ✅ Deep: timeline + orchestration metrics |
| Settings | 2 | M1 | ✅ Модели + permissions + MCP + tools + worker + metrics |
| Pull Requests | 6 | M5 | ✅ List/detail/create (`6.14`) |
| Memory | 6 | M5 | ✅ Panel + ask modal (`6.22`–`6.24`); Stage 7: resolve/pagination |
| Sites | 7 | M6 | ⬜ Stub UI — backlog `7.62`–`7.64` |
| Scheduled | 7 | M6 | ⬜ Templates only — backlog `7.65`–`7.67` |

---

## Матрица: инструменты × этапы

В матрице UI и каталоге инструментов доступны `filesystem.list` (этап 3) и `http.fetch` (этап 7, SSRF-safe redirects и ограниченный текстовый результат).

| Tool | Этап | Milestone | Статус |
| --- | --- | --- | --- |
| `filesystem.read` | 1 | M0 | ✅ |
| `filesystem.write` | 3 | M2 | ✅ Backend |
| `filesystem.patch` | 3 | M2 | ✅ Backend |
| `filesystem.search` | 3 | M2 | ✅ Backend |
| `shell.execute` | 3 | M2 | ✅ Backend |
| `git.status` | 4 | M3 | ✅ Backend |
| `git.diff` | 4 | M3 | ✅ Backend |
| `git.commit` | 4 | M3 | ✅ Backend |
| `git.pull` | 4 | M3 | ✅ Backend |
| `git.push` | 4 | M3 | ✅ Backend |
| `browser.open` | 6 | M5 | ✅ |
| `browser.extract` | 6 | M5 | ✅ |
| `mcp.call` | 6 | M5 | ✅ |
| `memory.search` | 6 | M5 | ✅ |
| `agent.run` | 7 | M6 | ✅ Subagent fan-out (`7.31`) |
| `worker.run` | 7 | M6 | ✅ Submit+await Python worker (`7.52`) |

---

## Принципы разработки

1. **Сначала vertical slice** — каждый этап начинается с минимального рабочего сценария
2. **Сервер — источник истины** — frontend только рендерит события
3. **Один PR — один milestone-deliverable** — не смешивать этапы
4. **Тесты до merge** — каждый tool и crate покрыт тестами
5. **Миграции обязательны** — любое изменение схемы БД через `migrations/`
6. **Протокол first** — сначала JSON Schema, потом Rust + TS

---

## Связанные документы

- [development-plan.md](development-plan.md) — полный план разработки
- [current-state.md](current-state.md) — что реализовано сейчас
- [architecture.md](architecture.md) — архитектура компонентов
