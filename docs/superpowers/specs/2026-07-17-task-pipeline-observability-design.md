# Task pipeline observability

> Дата: 2026-07-17  
> Статус: done (v1 metrics + v2 OTLP)  
> Scope: P1 metrics, then optional OpenTelemetry export

## Goal

Make session/task/tool/approval flow debuggable locally, and optionally export traces to an OTLP collector (Jaeger / Tempo / etc.).

## Design

### v1 — in-process

- **Correlation id** = `task_id` (stable across pause/resume/retry).
- Structured `tracing` events: `task.pipeline.*` with `correlation_id`, `session_id`, `task_id`, and latency fields.
- In-process `PipelineMetrics` on `AppState` (counters + average durations).
- `GET /api/metrics` returns a JSON snapshot (local operator debugging).

### v2 — OpenTelemetry (optional)

- Enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set and `OTEL_SDK_DISABLED` is not `true`.
- OTLP/HTTP span export via `opentelemetry-otlp` + `tracing-opentelemetry`.
- Pipeline task / tool / approval lifetimes are held as tracing spans (child tools/approvals under the task span).
- `GET /api/metrics` includes `otel_export_enabled`.
- Service name: `OTEL_SERVICE_NAME` (default `evohime-server`).

## Non-goals

- Persistent metrics storage
- Frontend metrics dashboard
- Required external collector for local development

## Acceptance

- Task start/finish, tool start/complete, approval wait→resolve, and task retry are counted
- Average task/tool/approval latency available in snapshot
- Unit tests cover collector math
- Without OTEL endpoint, behavior matches v1 (fmt tracing only)
- With OTEL endpoint configured, spans are created for pipeline operations
