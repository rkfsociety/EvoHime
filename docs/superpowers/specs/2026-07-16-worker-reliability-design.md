# Worker reliability (heartbeat, stall, schemas)

> Дата: 2026-07-16  
> Статус: approved (user waived per-section review)

## Цель

Закрыть `6.15`: Rust надёжно детектит мёртвый Python-worker и зависшие jobs, ретраит с backoff, а payload/result валидируются по task-схемам. Durability остаётся в Postgres у сервера; execution — в process-local Python worker.

## Порядок фаз

1. Process liveness (`GET /health` + Rust watchdog)
2. Per-job heartbeat + stall detection
3. Typed payload/result schemas

## Архитектура

- Pull-модель: Rust опрашивает worker.
- Python не знает про Postgres и не пушит heartbeat на server.
- При смерти процесса / смене `started_at` / stall job → auto-retry через существующий `retry_worker_job_after_error` + `max_attempts` + exponential backoff.
- Статус `stalled` — промежуточный в памяти poll-loop (ошибка для retry path), в PG job либо `running`/`retrying` во время попыток, либо terminal `failed`/`completed`.

## Data flow

### Submit

`POST /api/worker/jobs` → (фаза 3) validate → INSERT → submit worker → poll.

### Phase 1 — process watchdog

`worker_health_loop` каждые `WORKER_HEALTH_INTERVAL_SECS` (default 5):

- `GET /health` → `status`, `started_at`, `pid`, metrics, `supported_tasks`
- ok: обновить `last_seen_at` / `started_at`
- fail дольше `WORKER_HEALTH_STALE_SECS` (default 15) **или** сменился `started_at`: debounce + `recover_worker_jobs`

### Phase 2 — per-job stall

Пока job `running`, Python обновляет `heartbeat_at` ~раз в секунду.  
Rust poll: если `running` и `heartbeat_at` старше `WORKER_JOB_STALL_SECS` (default 30) → retry path с причиной stall.

### Phase 3 — schemas

Известные tasks: `echo`, `text.stats`, `text.keywords`.  
Валидация payload на Rust (до INSERT) и на Python (до queue). Result shape проверяется на Python перед `completed` (fail job если handler вернул мусор — для текущих handlers достаточно type checks в handler).

## Ошибки

| Ситуация | Поведение |
| --- | --- |
| Worker unreachable | health loop → recover/retry; poll `get` errors → retry |
| Worker restart (`started_at` change) | recover in-flight durable jobs |
| Job stall (no heartbeat) | retry with backoff until `max_attempts` |
| Bad payload | `400` от API / Python `400` на submit |
| Exhausted attempts | `failed` в PG |

## Тесты

- Python: health fields, heartbeat на running job, schema reject
- Rust: health client types, stall helper, payload validation, backoff (уже есть)

## Out of scope

- Push-heartbeat на Rust HTTP
- ML handlers сверх существующих
- Frontend UI для worker jobs
