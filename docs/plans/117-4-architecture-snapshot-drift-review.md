# План 117.4 — Architecture Snapshot & Drift Review: verification, release-evidence и закрытие

Статус: этап 4 для [плана 117.0](./117-0-architecture-snapshot-drift-review.md); после [плана 117.3](./117-3-architecture-snapshot-drift-review.md).

## Цель

Доказать требования issue #97 свежими воспроизводимыми тестами, оформить redacted release evidence и перенести только фактически реализованный контракт в каноническую документацию.

## Зависимости

### Блокирующие

- План 117.3 — полный Core/extraction/delta/IPC/UI vertical slice.
- Project release-evidence, `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md` и security/eval gates.

### Опциональные

- Plans 53, 75, 80 и 82 — дополнительные projections/evidence, не заменяющие базовую матрицу.

## Матрица проверки

- deterministic snapshot/hash на одинаковом revision, exact component/relationship evidence, all fact states, redaction и prompt-injection-as-data;
- manifest/process/route/store/worker/external extractors, unsupported diagnostics, coverage warnings, explicit omission binding и stale omission invalidation;
- added/removed/changed component/relationship, stable rename, ambiguous identity, boundary/trust crossing, evidence/coverage changes и deterministic delta hash;
- expected delta match, unexpected dependency, missing expected change, below-scope suppression, policy modes и bounded route semantics;
- incremental affected-evidence refresh, cancellation/timeout, schema/extractor/full-rebuild triggers, failed candidate preserving last-good и restart freshness recovery;
- multi-root same-path collision, unavailable root denial/no substitution, root grants and exact evidence authorization;
- IPC auth, bounds, replay/resync, duplicate/stale/idempotency, renderer forgery rejection и metadata-only UI projection;
- no secrets/raw source/prompts/outputs in labels, projections, logs or release bundle.

## Обязательные gates

1. Focused Core contract/storage/extractor/delta/recovery/security tests с migration backup, rollback и fault injection.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused architecture tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging.
4. `git diff --check`, redaction scan, dependency/provenance review и documented disable/rollback procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol/extractor versions, test IDs, snapshot/delta hashes, typed outcomes, coverage/omission summary и redaction status. Он не содержит credentials, raw source, prompts/outputs, absolute paths, PII или giant graph dumps.

После подтверждения всех criteria перенести фактический contract в `docs/architecture.md`, подтверждённое состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать recovery: last-good accepted snapshot остаётся доступен, failed/unknown refresh не объявляется success и не повторяет внешний effect вслепую.

После полного закрытия направления удалить комплект `117-0`…`117-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все критерии issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, extractor versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, rollback/recovery procedure записаны.

## Связанный issue

- [issue #97](https://github.com/rkfsociety/EvoHime/issues/97)
