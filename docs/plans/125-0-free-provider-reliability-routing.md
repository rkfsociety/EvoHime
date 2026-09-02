# План 125.0 — Free Provider Expansion & Reliability Routing

Статус: предложено по [issue #106](https://github.com/rkfsociety/EvoHime/issues/106). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Расширить существующий provider-profile слой набором free/free-limited cloud providers и добавить Core-owned runtime reliability, quota-safe probing и fail-closed failover. Новый слой не заменяет `ModelGateway` и не переносит routing authority в Electron.

```text
ProviderProfileRegistry -> dynamic catalog/free-state -> quota/limit snapshots
-> bounded reliability window -> capability/cost/privacy gate
-> reliability-aware resolver -> immutable snapshot -> existing ModelGateway
```

## Текущее основание и граница

В checkout уже есть `crates/model-gateway` с provider contract, catalog, routing policy/runtime и circuit/retry primitives, а в Core — model resilience policy и provenance. Они остаются владельцами транспорта, compatibility, retry policy и attempt evidence. Новый контракт должен расширить их additive-типами и устранить текущий выбор в основном по cost/latency; второй gateway и второй credential store запрещены.

Wave A: SambaNova Cloud, OVHcloud AI Endpoints, Z.ai, Scaleway, Alibaba DashScope/Model Studio, Requesty, Ollama Cloud. Wave B: Routeway, LLM7, Kilo, SiliconFlow, OpenCode Zen. Pollinations AI и другие неподтверждённые варианты — `Experimental`, выключены по умолчанию.

Каждый профиль обязан иметь stable id, transport kind, credential binding, endpoint/region, dynamic catalog, free-state source/TTL, capability mapping, rate-limit/usage parser, privacy metadata, probe policy и lifecycle state. OpenAI-compatible providers используют общий transport. Codestral остаётся моделью Mistral, если отдельного transport gap нет. Ни один provider не попадает в `FreeOnly` без подтверждённого zero-cost состояния для конкретной модели, региона и credential scope.

## Основной контракт

Ввести versioned типы `ProviderProfile`, `ProviderModelSnapshot`, `FreeAccessState`, `QuotaSnapshot`, `ProviderLimitSnapshot`, `ModelReliabilitySnapshot`, `ProbePolicy`, `ModelRouteCircuit`, `ReliabilityClass`, `RouteSelectionExplanation`, `RouteAttemptChain` и `FreeRoutingPolicy`. Reliability — это доступность endpoint, не benchmark качества; локальный Ollama и Ollama Cloud имеют разные identity и privacy boundary.

Порядок выбора: capabilities → explicit allow/deny → FreeOnly/PreferFree policy → credential/quota eligibility → circuit → reliability class → quality suitability → latency. Stale free-state становится `UnknownNeedsRefresh`, а paid/unknown никогда не является автоматическим fallback для `FreeOnly`.

## Этапы

- [Этап 1 — provider contract, catalog и storage](./125-1-free-provider-reliability-routing.md)
- [Этап 2 — reliability, probing, routing и failover runtime](./125-2-free-provider-reliability-routing.md)
- [Этап 3 — IPC, projection и UI](./125-3-free-provider-reliability-routing.md)
- [Этап 4 — verification, release evidence и закрытие](./125-4-free-provider-reliability-routing.md)

## Зависимости

### Блокирующие

- Existing `model-gateway` provider/routing contract, model resilience policy, provenance and credential registry boundaries.
- SQLite migration/backup, authenticated desktop IPC, Core event/replay and existing privacy/redaction guardrails.
- Live provider documentation or explicit fixture status for every advertised free/access mode; absence yields `Unknown`/`Experimental`, not an invented capability.

### Опциональные

- Agent Benchmark Matrix (#16) for quality suitability only.
- Local Model Performance Calibration (#101), diagnostics/evidence ledger and future provider-specific UI improvements.

## Критерии готовности

- [ ] Wave A/B profiles reuse transport and isolate credentials, regions, provenance and lifecycle metadata.
- [ ] Free state is volatile, scope-aware and fail-closed when stale or undocumented.
- [ ] Reliability metrics include bounded sample count, success/error rates, p50/p95, jitter/spike and confidence.
- [ ] Probes obey separate request/token budgets, adaptive cadence, quota floor and 429 cooldown.
- [ ] Routing ranks reliable compatible free candidates and preserves an immutable selected snapshot.
- [ ] Failover preserves every attempt, never silently enters paid mode, and does not retry non-idempotent effects.
- [ ] Core remains authority; Electron receives only bounded redacted explanation/projection.
- [ ] Focused and regression evidence covers provider isolation, volatility, reliability, circuit, quota and failover semantics.

## Non-goals

Central telemetry/rating, scraping provider websites as authority, guaranteed third-party free access, quality benchmark creation, direct edits to external CLI config, automatic paid fallback and support for every provider in one release.
