# План 125.4 — Verification, release evidence и закрытие

Статус: этап 4 для [плана 125.0](./125-0-free-provider-reliability-routing.md); после полного vertical slice.

## Зависимости

### Блокирующие

- План 125.3, Rust/storage/Core/IPC/Electron verification, redaction and release-evidence gates.
- Reproducible provider fixtures or documented live evidence for each enabled access mode; live quota must not be spent by tests unintentionally.

### Опциональные

- External provider credential tests in isolated opt-in environment; without credentials fixtures prove fail-closed behavior.

## Матрица проверки

- Profile/catalog: unique identities, shared transport, provider isolation, regional catalog, expiry/invalidation, migration/rollback/corruption.
- Free state: Free/FreeTierLimited/TrialCredits/Paid/Unknown/Experimental, stale-to-unknown, disappeared zero-price model and no paid FreeOnly fallback.
- Quota/probes: request/token budget, floor, anonymous/authenticated separation, adaptive cadence, Retry-After, scoped 429 cooldown and restart backoff.
- Reliability: deterministic p50/p95/jitter/spike, confidence/sample bounds, single-sample uncertainty, repeated timeout/5xx/429 circuits and HalfOpen recovery.
- Routing/failover: capability/privacy before score, reliable free candidate preference, immutable snapshot, selector semantics, full attempt chain, no non-idempotent retry.
- Security/privacy: no secrets/raw prompts/provider payloads in logs, storage, IPC, UI or evidence; local-only bounded telemetry.
- IPC/UI: replay/resync, forged authority rejection, redaction, accessible labels, distinct Unknown/Degraded/QuotaLimited/Paid states.

## Обязательные gates

1. Focused provider contract/storage/reliability/probe/routing/failover tests plus migration backup/rollback and fault injection.
2. Rust format, clippy, focused tests and appropriate workspace regression; Electron protocol/typecheck/tests/build/bundle checks.
3. Provider fixtures run offline by default; opt-in live smoke uses synthetic payloads, strict ceilings and isolated credentials.
4. `git diff --check`, encoding/redaction scan and evidence review. Report sample windows, fixture/config hashes, policy/catalog revisions and exact verdicts, never credentials or raw provider output.

## Release evidence и закрытие

После реализации перенести фактические provider/reliability/quota/failover invariants в `docs/architecture.md`, подтверждённое состояние и test totals в `docs/current-state.md`, а release procedure — в `docs/development-plan.md` и `docs/release-evidence.md`. Затем удалить комплект 125.0–125.4 по правилам каталога. До этого план остаётся implementation contract.

## Definition of Done

- [ ] Enabled provider profiles have evidence-backed access modes and isolated identity/credential/region.
- [ ] Free state, reliability, probes, circuits and failover are Core-owned and bounded.
- [ ] FreeOnly is fail-closed and every attempt remains in provenance.
- [ ] IPC/UI is metadata-only, replay-safe, redacted and accessible.
- [ ] Reproducible tests and release evidence cover all acceptance criteria.

## Связанный issue

- [#106 Free Provider Expansion & Reliability Routing](https://github.com/rkfsociety/EvoHime/issues/106)
