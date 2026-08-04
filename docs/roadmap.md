# EvoHime — Дорожная карта

> Этот документ содержит историческую web-first дорожную карту. Поддерживаемая продуктовая архитектура с 2026-08-04 — native Windows; актуальный план находится в `docs/development-plan.md`, архитектура — в `docs/architecture.md`.

> Обновлено: 2026-08-04. Ниже сохранена историческая web-first дорожная карта. Текущий native-план и его фактический статус указаны в разделе Native transition status и в `docs/development-plan.md`.

## Native transition status

| Native блок | Статус |
| --- | --- |
| Foundation: Rust Core, SQLite, versioned named-pipe IPC, supervisor, diagnostics | ✅ Завершён |
| WinUI shell: workspace, persistence, tray, notifications, reconnect/replay | ✅ Завершён |
| Agent workflow: streaming, cancellation, approval round-trip | ✅ Завершён |
| Native package, smoke build, Windows CI, removal of web product runtime | ✅ Завершён |
| Files, Editor, Git, controlled Terminal | ⬜ Следующий этап |
| Credentials, backup/restore, update/MSIX | ⬜ Запланирован |

Последний native-коммит: `87c5b39` (`feat: add native approval round-trip`).

## Обзор

```text
Этап 1 ✅ → 2 ✅ → 3 ✅ → 4 ✅ → 5 ✅ → 6 ✅ → Этап 7 🟡 → Этап 8 📝
Фундамент   Модель   Tools   Editor   Tasks   Advanced   Hardening + Product   Intelligence + DX
```

| Этап | Название | Статус | Ключевой результат |
| --- | --- | --- | --- |
| 1 | Фундамент | ✅ Готово | Vertical slice работает |
| 2 | Чат с моделью | ✅ Готово | LiteRouter streaming |
| 3 | Tools + shell | ✅ Готово | Sandbox, filesystem tools, shell, approvals, terminal, permissions |
| 4 | Editor + Git | ✅ Done | Browser file tree, Monaco editor, Git status/diff/actions, and synchronization events |
| 5 | Оркестрация | ✅ Complete | Lifecycle, команды, storage и recovery готовы |
| 6 | Advanced | ✅ Foundations done | Memory, PR, workers, observability, tool catalog |
| 7 | Hardening + Product | ✅ Complete | Security, reliability, Sites/Scheduled, agent 2.0, CI, DX — `7.107` (worktree-aware multi-checkout agent) closes the stage |
| 8 | Agent Intelligence + DX | 📝 Plan | Reasoning 2.0, experience/memory 3.0, subagent playbooks, plugin runtime 2.0, local reliability, UX/a11y |

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
| P1 | Native ReAct executor | ✅ `tool call → execute → observation → next action → respond` с bounded limits |
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

- `6.11` План-исполнитель с реальным графом шагов и повторным планированием ✅ (legacy compatibility; runtime использует native ReAct)
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
| 7.7 | Encrypt-at-rest для API keys в `app_settings` (или OS keychain) | M | ✅ | Phase 5.7: `crates/server/src/secrets.rs` AES-256-GCM; encrypt in `update_model_config`, decrypt in `startup.rs`; backward-compat fallback |
| 7.8 | Plugin install: pin commit/tag, signature/hash, uninstall/update | L | ✅ | `crates/storage/src/installed_plugins.rs` CRUD; `crates/server/src/plugins.rs` API; DB soft-delete with uninstalled_at; lock-file pin fields (backward compatible); integration tests |
| 7.9 | Plugin skills quarantine (не все skills → system prompt без opt-in) | L | ✅ | storage `plugin_skills` table + `get_disabled_skills()` / `toggle_skill_status()` / `list_plugin_skills()` CRUD; ReAct loop uses `build_workspace_rules_async()` with disabled skills filtering; UI toggle in PluginsPanel with enabled/disabled state indicators |
| 7.10 | Permission для `memory.search` + audit | S | ✅ | `Permission::MemorySearch` in enum, permission check in `execute_memory_search`, UI translation in SettingsPanel |
| 7.11 | Rate limiting / concurrency caps на sessions, tasks, worker jobs | M | ✅ | `rate_limit.rs`; 429 + WS `rate.limited`; `EVOHIME_RATE_LIMIT_*` |
| 7.12 | Git push/pull network policy (remote allowlist, deny force) | M | ✅ | `crates/tool-runtime/src/tools/git.rs` force validation + `EVOHIME_GIT_ALLOWED_REMOTES` allowlist; 5 unit tests |
| 7.13 | Content-Security-Policy / secure headers для static web | S | ✅ | `crates/server/src/secure_headers.rs`; Phase 5.6: CSP + X-Frame-Options + X-Content-Type-Options middleware |
| 7.14 | Secrets scan в CI (gitleaks / similar) | S | ✅ | Phase 5.5: `.github/workflows/rust.yml` gitleaks action |

### 7.B — Reliability & recovery

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.15 | Поднять Python worker из `start-dev.ps1` (+ tray icon) | M | ✅ | `-Worker`, tray, health wait `:8090`, auto-restart |
| 7.16 | LLM provider retry / backoff / retry-after | M | ✅ | `retry.rs` + literouter initial-request retries; `EVOHIME_LLM_*` |
| 7.17 | WebSocket reconnect + event resume (cursor / last event id) | M | ✅ | `HistoryItem` WS envelope; `after_sequence` / history `?after=`; frontend auto-reconnect; расширено в `7.116` (keyset pagination + exponential backoff) |
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
| 7.28 | Native provider `tool_calls` (OpenAI-compatible tools array) | L | ✅ | `chat_with_tools` + ReAct controller; bounded observations; `EVOHIME_NATIVE_TOOL_CALLS` |
| 7.29 | Распилить `agent_loop.rs` (plan / execute / context / parse) | L | ✅ | `agent_loop/{mod,parse,plan,execute,context,util}.rs` |
| 7.30 | Распилить `server/main.rs` на routers/modules | L | ✅ | `startup`/`routes`/`task/*`/`*_api`/`ws`; main ~64 LOC |
| 7.31 | Multi-agent / subagent fan-out с бюджетом | L | ✅ | tool `agent.run` + depth/concurrency/step/timeout budgets |
| 7.32 | Streaming tool progress (partial output events) | M | ✅ | `tool.output.delta` + shell stdout/stderr stream |
| 7.33 | Tool result truncation + summarization budget | M | ✅ | `tool_budget.rs`; head/tail + total chars env caps |
| 7.34 | Planner cost/latency telemetry per step | M | ✅ | GenAI OTLP spans + usage tokens + `/api/metrics` llm_* |
| 7.35 | `assistant.reply` + user-visible plan edits (approve plan) | M | ✅ | PostgreSQL checkpoint, edited dependencies, WebSocket approve/reject, durable paused state |
| 7.36 | More tools: `filesystem.list` в матрице UI; `http.fetch` с SSRF policy | M | ✅ | list surfaced in tool catalog/matrix; fetch registered with redirect and final-url SSRF checks |
| 7.37 | Cancel mid-tool with cooperative cancellation everywhere | M | ✅ | registry dispatcher cancels every tool future; shell receives token for child-process termination |
| 7.38 | Separate OpenAICompatible provider from LiteRouter alias | S | ✅ | standalone provider type/factory branch; `OPENAI_*` env configuration; LiteRouter routes remain backward-compatible |
| 7.39 | Model route picker в composer (не только Settings) | M | ✅ | route picker рядом с model picker; `model_route` отправляется из composer и сохраняется per-project |

### 7.D — Memory 2.0

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.40 | Выключить dual-write legacy `session_memory`/`global_memory` (migrate-only) | M | ✅ | post-task writes only `memory_items`; prompt via retrieve |
| 7.41 | Startup/one-shot `import_legacy_memory_notes` wired | S | ✅ | server startup; idempotent `source_label` markers |
| 7.42 | Semantic / fuzzy dedupe (не только fingerprint) | M | ✅ | kind-aware cosine dedupe over stored embeddings; conservative `0.58` threshold; exact fingerprint remains first |
| 7.43 | Conflict UI: side-by-side resolve / supersede flow | M | ✅ | atomic `POST /api/memory/:id` resolve; MemoryPanel side-by-side winner selection |
| 7.44 | Manual «добавить память» + templates | M | ✅ | POST /api/memory uses redaction/embedding/dedupe/conflict flow; MemoryPanel form with fact/preference/constraint/verification templates |
| 7.45 | Pagination / cursor для `/api/memory` (лимит 150) | M | ✅ | Stable keyset cursor по pinned/importance/updated_at/id; MemoryPanel дозагружает страницы по 50 |
| 7.46 | Memory delete confirm + undo window | S | ✅ | MemoryPanel confirmation plus 8-second Undo; restore reuses POST /api/memory admission flow |
| 7.47 | Local embedding model option (onnx / candle) без remote API | L | ✅ | `EVOHIME_EMBEDDING_MODE=local`; fastembed/ONNX BGE-small, MiniLM-L6 или multilingual E5; hash default + remote remain available |
| 7.48 | Experience playbook auto-suggest in ReAct | M | ✅ | Up to 3 relevant structured playbooks exposed as untrusted optional hints; no automatic execution |
| 7.49 | Memory export/import workspace pack (zip/json) | M | ✅ | GET /api/memory/export JSON/ZIP; POST /api/memory/import; MemoryPanel pack buttons + file import; imported rows enter candidate admission flow |
| 7.50 | Multi-device sync (out of earlier memory scope) | L | ✅ | Approved design: replica identity, append-only change log, cursor pull/push, snapshot recovery, offline outbox, conflict/tombstone rules; implementation split into 7.50a–e |

### 7.E — Workers & ML

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.51 | Ещё handlers: `text.classify`, `text.language`, `text.redact` | M | ✅ | Python worker + mirrored Rust validation; deterministic intent/language heuristics and memory-aligned secret redaction; exposed through `worker.run` |
| 7.52 | Agent tool `worker.run` (submit+await job) | M | ✅ | tool-runtime HTTP submit+poll; wiremock test |
| 7.53 | Worker job UI: submit form + payload editor в Settings | M | ✅ | WorkerSettingsSection: handler dropdown (7 handlers), JSON payload editor, submit form; API `submitWorkerJob()` |
| 7.54 | Horizontal worker scale (N processes / queue backend) | L | ✅ | distributed PostgreSQL queue via `/api/worker/queue/*` endpoints; `worker_distributed.py` polls and claims jobs atomically |
| 7.55 | Typed JSON Schema registry для worker tasks (shared) | M | ✅ | `workers/schemas/worker-tasks.schema.json` + Python jsonschema validation (with fallback) + Rust embedded schema + jsonschema crate validation; single source of truth for all task payloads |
| 7.56 | CI job для `workers/python` unittest | S | ✅ | `python-worker` job на Python 3.12 |

### 7.F — Project index & context

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.57 | Persistent on-disk index + incremental invalidate | L | ⬜ | full walk each search |
| 7.58 | Symbol / AST-aware chunks (tree-sitter optional) | L | ⬜ | path/symbol weights уже есть |
| 7.59 | Embeddings for project chunks (reuse memory embed pipeline) | L | ⬜ | comment «future encoder» |
| 7.60 | Sidebar global search (сейчас кнопка без handler) | M | ✅ | `/api/projects/search` endpoint + SearchModal UI + integration |
| 7.61 | `@file` / `@symbol` mentions в composer | M | ⬜ | attachments сейчас имена-only |

### 7.G — Product UI: Sites & Scheduled

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.62 | Sites: data model + `/api/sites` CRUD | L | ✅ | PostgreSQL + workspace-scoped API + real panel |
| 7.63 | Sites: preview / publish / open-in-browser | L | ✅ | Workspace-scoped HTML preview, publish action, and browser launch |
| 7.64 | Sites: search/filter wired to real data | S | ✅ | `GET /api/sites?q=&status=`; SitesPanel tabs + debounced search |
| 7.65 | Scheduled: real cron/timer jobs (storage + runner) | L | ✅ | `scheduled_tasks` PG table + tokio cron loop + CRUD API; 6-field cron via `cron` v0.17; atomic dispatch/idempotency and failure history implemented in migration `0024` |
| 7.66 | Scheduled: honest copy (убрать fake mail/calendar claims) | S | ✅ | removed fake mail/calendar templates; cron-only UI |
| 7.67 | Scheduled: list/pause/delete active schedules | M | ✅ | ScheduledPanel: list + tabs + create + pause/resume/trigger/delete |
| 7.68 | Deep-link / router для панелей (`?panel=`, history) | M | ✅ | `lib/panel-route.ts`; `?panel=` + pushState/popstate |

### 7.H — Frontend shell, UX, a11y

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.69 | Дораспилить `app.tsx` → hooks (`useWebSocket`, `useChat`, `useWorkspace`) | L | ✅ | socket transport/chat state/workspace file+Git runtime extracted; app 1968 LOC |
| 7.70 | CSS modules / split `styles.css` | L | ✅ | `styles.css` import-map + 8 ordered thematic files |
| 7.71 | React Error Boundary per panel | M | ✅ | `PanelErrorBoundary` вокруг основной панели и Settings modal |
| 7.72 | Орphan panels: Files/Git/Terminal/Tasks/Actions в навигации | M | ✅ | sidebar «Инструменты»: files/editor/terminal/git/tasks/actions |
| 7.73 | Реальные file attachments (upload + server store + tool access) | M | ✅ | session uploads API + DB metadata + workspace `.evohime/attachments` + prompt context |
| 7.74 | Chat: stable message keys, tool lines toggle, a11y labels | S | ✅ | `ChatLine.id` + toggle «Показать ход» + aria на чат/комposer |
| 7.75 | Settings modal: Escape, focus trap, tabpanel pattern | M | ✅ | `useModalA11y` + SettingsModal + tablist/tabpanel + ApprovalModal |
| 7.76 | Chat archive restore / unarchive | M | ✅ | `POST /api/sessions/:id/unarchive` + restore in Settings archive |
| 7.77 | Plugins: uninstall, update, skill browser | M | ✅ | uninstall/update API + skills preview в PluginsPanel |
| 7.78 | Approval modal: remember-path / temp-allow controls | M | ✅ | `remember_path` в protocol + UI «Один раз» / «Запомнить путь (1 ч)» |
| 7.79 | Silent boot errors → toast / Settings banner | M | ✅ | `BootNoticeBanner` + boot error collection в app startup |
| 7.80 | i18n consistency (RU/EN mix в Memory/Actions) | S | ✅ | `translateActionLabel/Detail` + русский MemoryPanel |
| 7.81 | Project chip: real git branch (не hardcoded `main`) | S | ✅ | `parseGitBranchFromStatus` + chip в app.tsx |
| 7.82 | Dead code cleanup (`addModelRoute` unused, placeholderPanel) | S | ✅ | удалены мёртвые route helpers и placeholder fallback |
| 7.83 | Show all chats (не `slice(0,5)` без «ещё») | S | ✅ | sidebar показывает все standalone-чаты |

### 7.I — Protocol, CI, DX

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.84 | CI: PostgreSQL service + storage/memory integration tests | L | ✅ | `postgres:16` service; `DATABASE_URL` + `EVOHIME_REQUIRE_DB`; `connect_integration_pool` fails hard in CI |
| 7.85 | CI: frontend `tsc` + build (+ optional playwright smoke) | M | ✅ | job `frontend` in CI: Node 22, `npm ci`, `typecheck`, `build`; Playwright deferred; server HTTP/WS integration harness закрыт в audit phase 4 |
| 7.86 | CI: protocol schema ↔ Rust ↔ generated TS drift check | M | ✅ | `protocol-drift` job regenerates TS and fails on diff |
| 7.87 | CI: Clippy `-D warnings` already; add fmt/docs gates docs for stage 7 | S | ✅ | fmt check is explicit; rustdoc runs with `RUSTDOCFLAGS=-D warnings` |
| 7.88 | Devcontainer / cross-platform launcher (не только Windows tray) | L | ✅ | `.devcontainer` Compose: workspace + PostgreSQL + Python worker |
| 7.89 | OpenAPI / typed HTTP client gen из server routes | L | ✅ | `generate:openapi` → `/openapi.json` с 98 route-level operations + typed `OpenApiPath`/method union; DTO-схемы остаются в domain API modules |
| 7.90 | Feature flags (`EVOHIME_FEATURE_*`) для experimental surfaces | M | ✅ | `/api/features`; Sites/Scheduled UI gates; OTLP export gate; server-side route gates закрыты в audit phase 3 |
| 7.91 | Docs sync: `development-plan.md` / `AGENTS.md` / `current-state` под Stage 7 | S | ✅ | синхронизированы с текущим Stage 7 |

### 7.J — Observability & ops

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.92 | Prometheus `/metrics` exposition (или OTEL metrics) | M | ✅ | done in 7.24 (`GET /metrics`); OTLP traces already optional |
| 7.93 | Per-request/request-id on HTTP (не только task correlation) | S | ✅ | `X-Request-Id` генерируется/пропагируется для каждого HTTP-ответа; internal details остаются в logs; query token ограничен WS handshake |
| 7.94 | Task timeline UI: correlation id copy + latency bars | M | ✅ | Tasks/Actions panels show server-provided task/action telemetry; correlation ids are copyable |
| 7.95 | Log sampling / redaction of secrets in tracing | M | ✅ | shared redaction helper protects internal/worker/OTLP dynamic fields; identical worker health failures sampled at configurable interval |
| 7.96 | Health endpoint: deep checks (DB, worker, disk) | S | ✅ | `GET /health/deep`; bounded parallel probes, safe component status, `503` on DB/disk failure |
| 7.97 | Backup/export: sessions+memory dump CLI | M | ✅ | `evohime-export --output <path>` writes versioned JSON with sessions, tasks/messages/events and structured memory |

### First implementation wave — code audit hardening

Эта волна была планом после `7.91` и полностью завершена. Порядок был фиксирован зависимостями: сначала устранение дублирования/двойных запусков, затем наблюдаемость и единый контур ошибок, после этого контракты и тестовая инфраструктура.

| Фаза | Deliverable | Основные файлы/границы | Статус | Критерий готовности |
| --- | --- | --- | --- | --- |
| 1 | **Scheduler correctness**: атомарный claim due-задач, lease/idempotency, связь scheduled run с созданной task, корректный success/failure count | `crates/storage/src/scheduled.rs`, `crates/server/src/scheduler.rs`, `crates/server/src/scheduled_api.rs`, `migrations/0024_scheduled_task_runs.sql` | ✅ | два scheduler-процесса не выполняют один run дважды; manual trigger и cron не перетирают состояние; race/failure integration tests |
| 2 | **Request context**: request-id, безопасные internal errors, header-only HTTP auth | `crates/server/src/auth.rs`, `api_error.rs`, `crates/server/src/request_id.rs`, WS handshake | ✅ | `X-Request-Id` есть на каждом HTTP-ответе; подробности internal errors остаются в logs; HTTP не принимает token из query, WS продолжает работать |
| 3 | **Feature and API contracts**: backend enforcement для Sites/Scheduled/OTLP и route-level OpenAPI/typed client | `crates/server/src/features.rs`, `routes.rs`, `sites_api.rs`, `scheduled_api.rs`, `otel.rs`, `scripts/generate-openapi.mjs`, `docs/openapi.json`, `.github/workflows/rust.yml`, `frontend/web/src/api/generated.ts` | ✅ backend enforcement (Forbidden 403 on all disabled features), OpenAPI generation (98 operations), CI drift check; DTO-схемы остаются в domain API modules |
| 4 | **Test and lifecycle foundation**: HTTP/WS integration harness, scheduler regression tests, graceful shutdown, bounded cleanup `session_buses` | `crates/server/tests/`, `crates/server/src/startup.rs`, `app.rs`, `ws.rs`, `scheduler.rs` | ✅ graceful shutdown ✅, session bus cleanup ✅, integration harness docs ✅, scheduler regression docs ✅, E2E database persistence tests ✅ (7 tests: session/task/event lifecycle, checkpoint pause/resume, operator scoping) |
| 5 | **Security and performance**: API-key encryption ✅, plugin pin/quarantine ✅, CSP/secure headers ✅, gitleaks ✅, frontend code splitting ✅ | `crates/server/src/models_api.rs`, `plugins.rs`, `auth.rs`, static serving, `.github/workflows/rust.yml`, `frontend/web/src/app.tsx` и panel imports, `vite.config.ts` | ✅ API keys encrypted (AES-256-GCM via `secrets.rs`), CI gitleaks action, secure headers (CSP/X-Frame-Options), plugin trust scoring + risk-scan gate + integrity lock, frontend lazy-loaded panels with React.lazy() + Suspense (initial bundle 62.65 kB, per-panel chunks 1–23 kB each) |

#### Правила выполнения первой волны

1. Каждая фаза — отдельный небольшой коммит с тестом и обновлением evidence в соответствующем roadmap item.
2. Для миграций сначала добавляется backward-compatible схема, затем код чтения/записи и только после этого cleanup старого пути.
3. Нельзя решать scheduler race только in-process mutex: защита должна жить в PostgreSQL и работать при нескольких server processes.
4. Нельзя скрывать Clippy/fmt проблемы через `#[allow]` или ослабление CI; исправления должны оставаться локальными и форматироваться workspace-правилом.
5. После каждой фазы обязательны: `cargo test --workspace --all-features --all-targets`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, frontend typecheck/build и generated drift checks.

**Фазы 1–5 выполнены:** scheduler correctness предотвращает повторное выполнение пользовательских задач, request context добавляет per-request id, Tasks/Actions показывают correlation id и server-provided latency, tracing redacts secrets и sample’ит повторяющийся worker шум, `/health/deep` проверяет DB, worker и workspace bounded-параллельно, feature gates enforced на backend, route-level OpenAPI генерируется из 98 операций, а CI проверяет drift, lifecycle, security и performance.

### 7.K — Moonshots / Stage 8 candidates

| # | Задача | Size | Статус | Notes / evidence |
| --- | --- | --- | --- | --- |
| 7.98 | Team / multi-operator mode (authz scopes) | L | ✅ | local operator registry, opaque tokens, owner/member management API и scoped sessions/tasks/memory |
| 7.99 | Cloud sync / remote workspace | L | ✅ | push/pull `BackupDump` через owner-only `/api/sync/*` с историей `sync_runs` (direction, лимит 64 MiB, checksum); идемпотентный restore (`restore_backup` + CLI `evohime-import`); авто-push по `EVOHIME_SYNC_AUTO_MINUTES` |
| 7.100 | Visual browser agent loop (CDP session reuse) | L | ✅ | `browser.session.navigate|read|click|type|screenshot|close` — persistent CDP-вкладка на задачу (`EVOHIME_BROWSER_CDP_URL`), кап 4 сессии, скриншоты в sandbox, ввод без эха текста, mock-CDP тесты без Chrome |
| 7.101 | Eval harness (golden tasks, regression agents) | L | ✅ | крейт `evohime-evals`: golden tasks против реального agent loop + tools; CI — детерминированный mock, CLI `evohime-eval --live --judge` — реальный провайдер + LLM-вердикты по `rubric` |
| 7.102 | Marketplace for playbooks / plugins with trust scores | L | ✅ | trust score из проверяемых сигналов + UI-бейджи; install-гейт со статическим risk scan (отказ ниже-official без `force`); lock-файл с content-hash и `/api/plugins/integrity` (ok/modified/unlocked/missing) + integrity-chip. Криптоподписи/репутация авторов — вне пункта: внешней PKI у OSS-каталогов нет |
| 7.103 | Online continual learning (still no weight fine-tune; stronger experience) | L | ✅ | wave 1 ✅: обучение на провалах — ограниченная полоса extract (≤2 кандидата `failure_pattern`/`verification_rule`, scope experience, confidence cap 0.6 → только Ask-гейт, без auto-promote); harmful-фидбек использованной памяти уже был; wave 2 ✅: эскалация confidence/importance при повторе через `FeedbackSignal::Repeated` (confidence кап 0.6 не снимается) и retrieval-бонус для failure_pattern/verification_rule над success_pattern/playbook |
| 7.104 | Mobile-responsive shell (browser-only, no native app) | M | ✅ | off-canvas sidebar/trace-panel drawers через CSS media query `≤768px` (гамбургер, общий backdrop, закрытие по Escape/клику вне/выбору пункта), touch-таргеты ≥44px; десктопная раскладка не тронута |
| 7.105 | Voice input / TTS optional | M | ✅ | Browser-only Web Speech API: `useVoiceInput` with secure-context/error lifecycle and `ru-RU` transcript handling; centralized `useSpeechSynthesis`; accessible composer/message controls; commits `aeec3af`, `5762c52`, `b447c42`, `3525369` |
| 7.106 | Diff review UI for agent patches before apply | L | ✅ | 128 KiB UTF-8 preflight; typed optional `approval.required.review`; exact server-authored unified diff; shared renderer; apply-once/deny modal without remember-path; commits `e2b6a1e`, `086bf3c`, `179c6ad`, `0c687b3`, `03f2303` |
| 7.107 | Worktree-aware multi-checkout agent (parallel tasks isolated) | L | ✅ | `task_worktrees` table; detached-HEAD worktrees under OS temp dir, provisioned atomically alongside `task_cancellations` (idempotent, rolled back on DB failure, entries kept alive through approval pauses, `TaskCancel`, `TaskPlanReject`, `TaskRetry`'s stale-worktree teardown, a server restart, and a panic via `TaskCancellationGuard`); path-scoped squash merge-back (`git apply --3way --index` + scoped `commit`/`checkout HEAD` restore, never a blanket reset/commit) under a per-workspace `workspace_merge_locks` registry, serialization verified under real concurrent execution; startup + hourly cleanup keyed to live task status with retention-overflow, orphan-sweep, and cascade-orphan handling |
| 7.108 | Cost budgets & spend caps per day/model | M | ✅ | Backend (Wave 1): `cost_limits` table (`crates/storage/src/cost_limits.rs`), `/api/models/cost-limits` CRUD; Frontend (Wave 2): `SpendSettingsSection.tsx` в Settings, gauge визуализация, edit/save flow; коммит `2f3257e` |
| 7.109 | Self-update channel for launcher | M | ✅ | `crates/launcher/src/self_update.rs` — скачивание нового `launcher.exe` во временный путь с проверкой SHA256, эстафета отдельному `updater.exe` процессу (файл лаунчера заблокирован пока сам работает); `crates/updater`; `LauncherStatus.tsx` панель статуса в Settings; коммиты `b9329b6`, `73bf411` |
| 7.110 | Formal threat model doc + abuse cases | M | ✅ | `docs/security/threat-model.md` + `SECURITY.md`; all 7.A–7.E пункты documented |
| 7.111 | Extended reasoning: Claude thinking (streaming, budget, cost-tracking) | L | ✅ | Model gateway (`ChatStreamItem::Thinking`, SSE parser), `thinking_settings`/`thinking_usage` таблицы, `GET/PUT /api/settings/thinking`, `ThinkingSettingsSection.tsx`, event batching в agent runtime (`ServerEvent::AgentThinking`), eval harness `thinking_contains`/`min_thinking_tokens`, provider `supports_thinking` detection; см. `docs/wave-3b-extended-reasoning.md`; 11 коммитов от `e78ab74` до `8680eba` |
| 7.112 | Project Context 2.0: semantic search via embeddings | M | ✅ | `crates/project-index`: детерминированные 384-dim embeddings, hybrid lexical+semantic scoring (BM25 + cosine), symbol-aware weighting, path hierarchy boost; 16 unit-тестов; см. `docs/wave-4-plan.md`; коммиты `b06e3ae`, `1d1d980`, `ef27380` |
| 7.113 | Plugin marketplace: audit trail для install/update/uninstall/pin | M | ✅ | Дополняет `7.102`: таблица `plugin_audit` (`migrations/0033`), DAO `crates/storage/src/plugin_audit.rs`, событие `force_override` при обходе risk-scan, `GET /api/plugins/audit`, секция "История действий" в `PluginsPanel.tsx`; коммит `cf44374`. Вне скоупа: Ed25519-подписи авторов, community reputation |
| 7.114 | OTLP metrics export + `/metrics/history` frontend trends | S | ✅ | Дополняет `7.92`/`7.24`: `crates/server/src/otel.rs` — `MetricExporter`/`SdkMeterProvider`, `register_pipeline_metrics()` зеркалит Prometheus-метрики через OTLP (no-op если выключено); sparkline-графики в `MetricsSettingsSection.tsx` для существующего `/api/metrics/history`; коммит `e4aea1c` |
| 7.115 | Frontend performance: sourcemaps, error traces, Lighthouse pass | S | ✅ | `build.sourcemap: true`; `PanelErrorBoundary.tsx` показывает stack+componentStack с копированием; `globalErrorHandlers.ts` (`window.onerror`/`unhandledrejection`); `TerminalPanel` переведён на `React.lazy()`; кастомный vite-plugin `deferNonCriticalCss` устранил render-blocking `vendor-monaco.css`; meta description + robots.txt; Lighthouse 95/100/96/100; коммит `6f28e01` |
| 7.116 | Session recovery: keyset pagination + WS reconnect resiliency | M | ✅ | Расширяет `7.17`: cursor-based keyset pagination для `GET /api/sessions/:id/history` (`PaginatedEventsCursor`, forward/backward `order`, backward-compatible с `after`); `useWebSocket` — exponential backoff+jitter, max 5 попыток, state machine idle→connecting/reconnecting→connected/failed, `sessionStorage` контекст переподключения (30 мин); `useSessionReplay` для paginated backfill истории; коммиты `7b0d0ca`, `c37f912`, `5ee8939` |

### Suggested Stage 7 delivery waves

**Актуальный статус 2026-07-29:** `7.93`–`7.106` ✅ — request context, task timeline telemetry, log safety, deep health checks, backup/export, multi-operator authz, cloud sync, browser sessions, eval harness, plugin trust/integrity, failure learning, mobile shell, voice/TTS и diff review UI; `7.108`–`7.116` ✅ — cost limits, self-update, extended reasoning, semantic project search, plugin audit trail, OTLP trends, frontend performance и session recovery.

**Актуальный статус 2026-07-30:** `7.105`–`7.116` подтверждены кодом; OpenAPI-контракт содержит 98 операций. Session recovery отслеживается как `7.116`, а `7.109` — как self-update launcher. `7.107` (worktree-aware multi-checkout agent) закрыт — **Stage 7 полностью завершён**.

1. **Wave A (trust):** `7.1`–`7.6`, `7.11`, `7.15`–`7.16` ✅
2. **Wave B (survive restarts):** `7.17`–`7.27`, `7.40`–`7.41` ✅
3. **Wave C (agent quality):** `7.28`–`7.39`, `7.42`–`7.51`, `7.52` ✅
4. **Wave D (product honesty):** 7.62–7.68, 7.72–7.73 ✅
5. **Wave E (DX/CI):** `7.56`, `7.69`–`7.71`, `7.84`–`7.98` ✅
6. **Wave F (scale/moonshots):** 7.54 ✅, `7.57`–`7.59` ⬜, `7.98`–`7.116` ✅ (включая `7.107`)

### Критерий готовности Stage 7 (минимум)

- Локальный сервер по умолчанию не торчит в LAN без auth  
- SSRF blocked для browser/MCP  
- Launcher поднимает Python worker  
- WS reconnect не теряет критичные события  
- Legacy memory dual-write выключен или явно deprecated  
- Sites/Scheduled либо реализованы, либо убраны из «как будто работают»  
- CI гоняет frontend + Postgres integration + Python worker tests  
  (`7.84` Postgres ✅; `7.85` frontend ✅; worker/`7.56` ✅)

---

## Этап 8 — Agent Intelligence, Plugin Runtime 2.0 & Local Excellence

**Цель:** после закрытия Stage 7 (trust boundary, restarts, product surface) следующий слой — качество самого агента (reasoning, память, playbooks), более безопасная и удобная plugin-экосистема, и локальная надёжность/DX/UX без ухода в multi-tenant/enterprise/SaaS-территорию.

**Явно вне скоупа и намеренно не включено из черновика:** SSO/SAML/AD, multi-tenant organization hierarchy, SOC2/GDPR compliance pack, Kubernetes autoscaling, service mesh/zero-trust, blue-green/canary deploy, billing, AR/VR/holographic/BCI/quantum-inspired UI — всё это противоречит принципу single-tenant локального инструмента (см. правила памяти в `6.16`–`6.25`) и добавлено бы просто как SaaS-балласт.

### 8.A — Agent reasoning & planning 2.0

**Зависимости:** native ReAct (`7.28`), subagent fan-out (`7.31`), experience memory (`6.21`, `7.48`)

| # | Задача | Size | Статус | Notes / rationale |
| --- | --- | --- | --- | --- |
| 8.1 | Tree-of-Thoughts bounded planner (branch + prune перед выполнением) | L | ✅ | protocol `agent.plan` + DAO `crates/storage/src/planning_history.rs` + unified scoring formula (similarity + tool success + complexity + feedback) + deterministic pruning to top-N + fallback on error + history with 30-day TTL + `AgentPlanView` frontend component + E2E test |
| 8.2 | Self-reflection loop: агент проверяет собственный шаг перед следующим и пересматривает план при ошибке | L | ✅ | `ReflectionStage` вызывается в ReAct-цикле после каждого наблюдения инструмента (`crates/agent-runtime/src/agent_loop/react.rs`): подтягивает `failure_pattern`/`verification_rule` из experience memory (`6.21`), матчинг по перекрытию значимых токенов (а не подстрокой), пишет строку в `reflection_events`, шлёт `agent.reflection` в WS и добавляет hint в наблюдение для модели; `RetryTool` разблокирует дублирующий вызов в рамках retry-бюджета, 3 провала подряд → `RevisePlan` + фаза `revising_plan`; UI — `ReflectionTimeline.tsx`; выключатель `EVOHIME_REFLECTION_ENABLED=0`. Вне скоупа пункта: блокирующий ask-gate (`8.4`) и автоматический перезапуск планировщика `8.1` |
| 8.3 | Явный граф зависимостей задач при декомпозиции (вместо линейного плана) | L | ✅ | Kahn O(V+E) topological sort, cycle detection, backward-compat materialization (legacy linear plans), versioned DB schema, batch computation via topological depth, React Flow DAG viewer with real-time status, cumulative failure tracking (max 3), 7 E2E integration tests covering linear/diamond/cyclic/complex pipelines |
| 8.4 | Meta-cognitive confidence сигнал в ask-gate (шире, чем текущий uncertainty-порог) | M | ✅ | 4-сигнальная агрегация (модель/опыт/tool stats/reflection), risk-aware ask/require пороги, `agent.confidence` WS-событие, `ConfidenceAndRisk` + `ForceApproveModal` в UI. См. `docs/features/confidence-ask-gate.md` |
| 8.5 | Counterfactual dry-run для high-impact tool calls перед approval | M | ✅ | Расширяет существующий синхронный approval-preview (`ApprovalReview`): `crates/tool-runtime/src/risk.rs` классифицирует резолвленный вызов инструмента (`ToolRiskLevel` None/Low/Medium/High); `filesystem.write` получает точный предикт (create/overwrite + размеры) через тот же `WorkspaceSandbox`, что и реальное исполнение; всё прочее — честный `Unavailable{reason}` вместо угадывания; `risk_level` в `ApprovalRequired` событии; без новой БД/эндпоинта/кеша — вычисляется синхронно при формировании approval. Подтверждено живым end-to-end тестом через агента. См. `docs/superpowers/specs/2026-08-03-stage-8-5-counterfactual-dryrun-design.md` |
| 8.6 | Аналогичный retrieval: явное переиспользование шагов из похожих прошлых задач | M | ⬜ | расширяет playbook auto-suggest (`7.48`) |

### 8.B — Experience & memory 3.0

**Зависимости:** memory pipeline (`6.16`–`6.25`), feedback loop (`6.23`), eval harness (`7.101`)

| # | Задача | Size | Статус | Notes / rationale |
| --- | --- | --- | --- | --- |
| 8.7 | Imitation-сигнал из ручных правок пользователя после ответа агента | M | ⬜ | новый источник experience, не требует fine-tune весов |
| 8.8 | Автоподбор few-shot примеров под тип задачи из experience memory | M | ⬜ | |
| 8.9 | Локальный A/B-харнесс для системных промптов поверх eval harness | M | ⬜ | использует `7.101`, без облачной телеметрии |
| 8.10 | Active learning: подсветка memory-кандидатов с наименьшей уверенностью для ревью | S | ⬜ | расширяет MemoryPanel |
| 8.11 | Auto-archive устаревшей памяти при длительном confidence decay | S | ⬜ | расширяет feedback decay (`6.23`) |
| 8.12 | Опциональный cross-project experience (только по явному opt-in, выключено по умолчанию) | M | ⬜ | не должен нарушать изоляцию project/workspace памяти |

### 8.C — Specialized subagent playbooks

**Зависимости:** `agent.run` subagent fan-out (`7.31`), eval harness (`7.101`)

| # | Задача | Size | Статус | Notes / rationale |
| --- | --- | --- | --- | --- |
| 8.13 | Playbook: безопасный refactor (rename/extract), верификация тестами до commit | M | ⬜ | |
| 8.14 | Playbook: генерация unit/integration тестов из diff | M | ⬜ | |
| 8.15 | Playbook: локальный security review (pattern-based, без облачной отправки кода) | M | ⬜ | |
| 8.16 | Playbook: профилирование производительности (обёртка над flamegraph/hyperfine) | M | ⬜ | |
| 8.17 | Playbook: генерация docs/changelog из diff | S | ⬜ | |
| 8.18 | Playbook: апдейт зависимостей (semver-aware, тесты перед commit) | M | ⬜ | |

### 8.D — Plugin runtime 2.0

**Зависимости:** plugin install/pin/quarantine (`7.8`–`7.9`), marketplace trust scores (`7.102`)

| # | Задача | Size | Статус | Notes / rationale |
| --- | --- | --- | --- | --- |
| 8.19 | WASM sandbox для выполнения плагинов вместо текущего shell-based пути | L | ⬜ | усиливает `7.9` quarantine |
| 8.20 | Plugin SDK + scaffold CLI (`evohime plugin init`) | M | ⬜ | |
| 8.21 | Hot-reload плагина при разработке (без рестарта сервера) | S | ⬜ | |
| 8.22 | Versioned plugin API с deprecation warnings | M | ⬜ | |
| 8.23 | Per-plugin permission sandboxing (filesystem/network/shell scopes) | M | ⬜ | расширяет `7.9` |

### 8.E — Local reliability, perf & ops

**Зависимости:** backup/export (`7.97`), retry/backoff (`7.16`), deep health (`7.96`), telemetry (`7.34`)

| # | Задача | Size | Статус | Notes / rationale |
| --- | --- | --- | --- | --- |
| 8.24 | Point-in-time recovery через локальный WAL archiving PostgreSQL | M | ⬜ | расширяет `7.97` backup/export |
| 8.25 | Периодическая автоматическая проверка бэкапа через restore-тест | S | ⬜ | |
| 8.26 | Circuit breaker для LLM/MCP/`http.fetch` вызовов | M | ⬜ | расширяет `7.16` retry/backoff |
| 8.27 | Graceful degradation при недоступности DB/worker/LLM | M | ⬜ | |
| 8.28 | Dashboard здоровья зависимостей (LiteRouter, MCP servers, worker) | S | ⬜ | расширяет `7.96` deep health |
| 8.29 | Локальный кэш для project-index / memory retrieval (in-process, без внешнего Redis) | M | ⬜ | |
| 8.30 | Ревизия производительности запросов + index tuning pass | S | ⬜ | |
| 8.31 | Cost/token usage аналитика по задаче и модели | S | ⬜ | расширяет `7.34` planner telemetry |
| 8.32 | Anomaly detection по локальным метрикам (spend/latency spikes) | M | ⬜ | |

### 8.F — UX, accessibility & DX surface

**Зависимости:** frontend shell decomposition (`7.69`–`7.71`), multi-device sync (`7.99`)

| # | Задача | Size | Статус | Notes / rationale |
| --- | --- | --- | --- | --- |
| 8.33 | Command palette (Ctrl+K) | M | ⬜ | |
| 8.34 | Редактор keyboard shortcuts | M | ⬜ | |
| 8.35 | Notification center с историей | S | ⬜ | |
| 8.36 | Полный проход по screen reader / keyboard-only навигации | L | ⬜ | |
| 8.37 | High-contrast режим, font scaling, reduced-motion настройка | S | ⬜ | |
| 8.38 | Сохранение/восстановление кастомных раскладок панелей | M | ⬜ | опирается на `7.99` multi-device sync для переноса между устройствами |
| 8.39 | VS Code extension (тонкий клиент поверх существующего HTTP/WS API) | L | ⬜ | |
| 8.40 | `evohime-cli` для headless запуска задач в CI/CD | M | ⬜ | |
| 8.41 | GitHub Actions integration (запуск EvoHime task из workflow) | M | ⬜ | опирается на существующий PR workflow (`6.14`) |
| 8.42 | Интерактивный onboarding-тур внутри продукта | M | ⬜ | |
| 8.43 | Cookbook типовых локальных сценариев | S | ⬜ | |

### Критерий готовности Stage 8 (минимум)

- Агент умеет самостоятельно распознавать и исправлять собственные ошибки в рамках задачи (self-reflection), не только полагаться на ask-gate.
- Плагины выполняются в изолированном WASM sandbox с явными permission scopes, а не в общем shell-контексте.
- Локальный бэкап проверяется автоматически, а не только создаётся.
- Хотя бы один внешний DX-поверхностный канал (VS Code extension или `evohime-cli`) закрывает разрыв между веб-UI и повседневным workflow разработчика.
- Ничего из explicitly-out-of-scope списка (SSO/multi-tenant/SOC2/AR-VR/quantum) не просочилось в реализацию как "заодно".

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
| Sites | 7 | M6 | ✅ CRUD + preview/publish + search/filter (`7.62`–`7.64`) |
| Scheduled | 7 | M6 | ✅ Real cron jobs: storage + runner + CRUD + panel (`7.65`–`7.67`) |

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
| `browser.session.navigate` | 7 | M6 | ✅ Persistent CDP tab (`7.100`) |
| `browser.session.read` | 7 | M6 | ✅ Persistent CDP tab (`7.100`) |
| `browser.session.click` | 7 | M6 | ✅ Persistent CDP tab (`7.100`) |
| `browser.session.close` | 7 | M6 | ✅ Persistent CDP tab (`7.100`) |
| `browser.session.type` | 7 | M6 | ✅ Persistent CDP tab (`7.100`) |
| `browser.session.screenshot` | 7 | M6 | ✅ PNG в workspace sandbox (`7.100`) |

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
