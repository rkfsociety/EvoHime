# Task pipeline observability

> Дата: 2026-07-17  
> Статус: approved (execute next)  
> Scope: P1 — correlation id + structured logs + in-process metrics

## Goal

Make session/task/tool/approval flow debuggable without an external APM stack.

## Design

- **Correlation id** = `task_id` (stable across pause/resume/retry).
- Structured `tracing` events: `task.pipeline.*` with `correlation_id`, `session_id`, `task_id`, and latency fields.
- In-process `PipelineMetrics` on `AppState` (counters + average durations).
- `GET /api/metrics` returns a JSON snapshot (local operator debugging).

## Non-goals (v1)

- OpenTelemetry / Jaeger export
- Persistent metrics storage
- Frontend metrics dashboard

## Acceptance

- Task start/finish, tool start/complete, approval wait→resolve, and task retry are counted
- Average task/tool/approval latency available in snapshot
- Unit tests cover collector math
