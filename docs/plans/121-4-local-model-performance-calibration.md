# План 121.4 — Local Model Performance Calibration: verification, release-evidence и закрытие

Статус: этап 4 для [плана 121.0](./121-0-local-model-performance-calibration.md); после [плана 121.3](./121-3-local-model-performance-calibration.md).

## Цель

Подтвердить требования issue #101 свежими воспроизводимыми тестами, оформить redacted release evidence и перенести фактически реализованный contract в canonical documentation.

## Зависимости

### Блокирующие

- План 121.3 — полный Core/runner/integration/IPC/UI vertical slice.
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`, release-evidence и security/eval gates.

### Опциональные

- Agent Benchmark Matrix linkage, diagnostics, optional telemetry and CLI.

## Матрица проверки

- exact model artifact/runtime/version/hardware/driver/launch/context/suite identity; no display-name reuse; stale/foreign profile behavior;
- cold load, warmup exclusion, measured samples, deterministic fixtures, TTFT/prefill/decode/latency/tokens and Unknown telemetry;
- context curve within #116 safe ceiling, headroom confidence, smaller context retained after larger-context OOM, memory/contended measurement handling;
- median/p10/p90, variance classes, failure rate, insufficient samples, invalid sample exclusion and derived performance classes;
- calibration session lifecycle, cancellation/timeout/runtime crash/OOM, resource conflict with normal call, supervisor cleanup and restart reconciliation;
- exact measurement applied to #96 recommendation and #95 purpose routing only within allowed candidates; fast incompatible model loses, quality/security/budget remain dominant and explicit selection is not silently replaced;
- same model across runtimes comparison with per-purpose tradeoffs, optional #16 provenance without duplicated low-level runner;
- IPC auth/bounds/replay/resync/idempotency, renderer fake-result rejection, metadata-only UI and clear measured/estimated/stale/not-measured states;
- no network sharing, hardware identifiers, model inventory, credentials, workspace grants, raw prompts/outputs or arbitrary executable args in storage/logs/IPC/release bundle.

## Обязательные gates

1. Focused Core/storage/aggregation/runner/runtime/recovery/security tests with migration backup, rollback and fault injection; real-runtime tests only where a verified fixture/runtime is available, otherwise explicit unavailable outcome.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused calibration tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging/runtime delivery.
4. `git diff --check`, redaction/privacy scan, dependency/provenance review и documented disable/retention/recovery procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol/runtime/suite versions, test IDs, exact identity hashes, aggregate/typed outcomes, unavailable reasons and redaction status. Не включать raw prompts/outputs, hardware serials/PII, credentials, model bytes, unrestricted traces или automatic upload payloads.

После подтверждения criteria перенести фактический contract в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать rollback: отключение calibration оставляет #96 normal inference/recommendation estimates работоспособными; unknown/failed benchmark не объявляется success и не ломает installed runtime.

После полного закрытия направления удалить комплект `121-0`…`121-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, runtime/suite versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, rollback/retention/recovery procedure записаны.

## Связанный issue

- [issue #101](https://github.com/rkfsociety/EvoHime/issues/101)
