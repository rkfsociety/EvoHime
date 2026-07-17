# Worker observability

> Дата: 2026-07-17  
> Статус: done

## Цель

Сделать Python-worker bridge наблюдаемым без отдельного APM: counters, last health, list recent jobs, structured logs.

## Design

- In-process `WorkerMetrics` on `AppState` (submit / complete / fail / retry / stall / health / recovery).
- Structured `tracing` events: `worker.pipeline.*`.
- `GET /api/worker/status` — metrics snapshot + DB status counts.
- `GET /api/worker/jobs?limit=` — recent durable jobs (default 50, max 200).
- `GET /api/metrics` includes nested `worker` alongside `pipeline`.

## Non-goals

- Frontend worker dashboard
- Push metrics to Prometheus (OTLP traces already optional at process level)
