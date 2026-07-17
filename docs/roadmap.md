# EvoHime — Дорожная карта

> Обновлено: 2026-07-16

## Обзор

```text
Этап 1 ✅ ──→ Этап 2 ──→ Этап 3 ──→ Этап 4 ──→ Этап 5 ──→ Этап 6
Фундамент     Модель      Tools       Editor      Tasks       Advanced
```

| Этап | Название | Статус | Ключевой результат |
| --- | --- | --- | --- |
| 1 | Фундамент | ✅ Готово | Vertical slice работает |
| 2 | Чат с моделью | ✅ Готово | LiteRouter streaming |
| 3 | Tools + shell | ✅ Готово | Sandbox, filesystem tools, shell, approvals, terminal, permissions |
| 4 | Editor + Git | ✅ Done | Browser file tree, Monaco editor, Git status/diff/actions, and synchronization events |
| 5 | Оркестрация | ✅ Complete | Lifecycle, команды, storage и recovery готовы |
| 6 | Advanced | ✅ Foundations done; optional backlog / polish |

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
- **Next:** optional backlog / reliability polish

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
| Settings | 2 | M1 | ✅ Модели + permissions + MCP + tools |
| Pull Requests | 6 | M5 | ✅ List/detail/create (`6.14`) |

---

## Матрица: инструменты × этапы

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
