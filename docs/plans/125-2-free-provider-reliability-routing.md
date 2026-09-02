# План 125.2 — Reliability, probing, routing и failover runtime

Статус: этап 2 для [плана 125.0](./125-0-free-provider-reliability-routing.md); после 125.1.

## Зависимости

### Блокирующие

- План 125.1, existing `RoutePolicySnapshot`/`RunHealthOverlay`, model resilience policy, cancellation, circuit and provenance paths.

### Опциональные

- Benchmark suitability and diagnostics; without them routing uses capability/policy/reliability only and reports the missing signal.

## Реализация

1. Реализовать bounded rolling observations по `provider_id + model_id + route_fingerprint`: sample count/window, success/timeout/429/5xx rates, p50/p95, jitter, spike rate, consecutive failures, cooldown, confidence and observed time. Single fast sample never yields `Healthy`.
2. Ввести typed `ReliabilityClass`: `Excellent`, `Healthy`, `Degraded`, `Unstable`, `CoolingDown`, `QuotaLimited`, `Unavailable`, `Unknown`; thresholds versioned/Core-owned and distinct from quality benchmark.
3. Реализовать probe scheduler с `PassiveOnly`, `AnonymousWhenPossible`, `AuthenticatedBounded`, `ActivePriority`, idle/failure backoff, request/token budgets, quota floor and adaptive cadence. 429 or exhausted quota stops active probes for affected scope.
4. Нормализовать Retry-After conservatively, classify 401/403 as credential failure, open model/account/endpoint-scoped circuits for repeated timeout/5xx/429/unavailable/protocol mismatch, and persist enough backoff/circuit state to avoid restart storms.
5. Расширить resolver: capabilities/privacy/policy/quota/circuit first, then reliability class and p95/jitter/spike, then quality/latency. `FreeOnly` has no paid escape hatch; PreferFree requires explicit fallback policy.
6. Resolve virtual selectors (`FreeReliable`, `FreeFastReliable`, `FreeCodingReliable`, `FreeToolsReliable`, `FreeLongContextReliable`) to immutable snapshots before a call; active streams never migrate.
7. Реализовать idempotent inter-call failover with bounded attempts. Preserve full redacted `RouteAttemptChain`, upstream provenance and failure reasons; do not retry non-idempotent effects without explicit semantics.
8. Интегрировать runtime observations with existing gateway and provenance without persisting provider secrets/raw payloads or creating central telemetry.

## Критерии выхода

- [ ] Metrics and classes are deterministic over bounded windows.
- [ ] Probes cannot exceed configured quota budgets/floors.
- [ ] Routing is explainable and reliability-aware while preserving capability/privacy gates.
- [ ] Circuits, cooldowns and failover provenance are bounded, replay-safe and restart-safe.

## Не входит

Provider UI, external telemetry, quality scoring, arbitrary provider scraping and non-idempotent operation migration.
