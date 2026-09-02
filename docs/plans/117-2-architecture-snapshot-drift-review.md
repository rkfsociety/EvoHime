# План 117.2 — Architecture Snapshot & Drift Review: extraction, runtime и recovery

Статус: этап 2 для [плана 117.0](./117-0-architecture-snapshot-drift-review.md); после [плана 117.1](./117-1-architecture-snapshot-drift-review.md).

## Цель

Построить snapshot из repository evidence, проверить candidate facts Core-side, выполнить deterministic delta и incremental refresh с bounded cancellation, freshness и recovery.

## Зависимости

### Блокирующие

- План 117.1 — immutable contract, storage, hashes, states и validation.
- Semantic Repository Map/evidence index, workspace root grants, event journal, model structured-response contract и existing change protocol.

### Опциональные

- Context References/Project Instruction Stack для дополнительных reviewed metadata.
- Diagnostics bundle для расширенного redacted trace.

## Реализация

1. Реализовать Core-owned extractor registry с `id`, `version`, supported inputs, emitted fact kinds и confidence class. MVP: Cargo/npm/workspace manifests, executable/process entry points, protobuf/IPC, routes, schemas/stores, schedulers/workers, message bindings и explicit project architecture metadata.
2. Связать extractor outputs с Map snapshot/evidence index. Не выполнять scripts/plugins/repository binaries; unsupported input даёт explicit coverage diagnostic.
3. Добавить bounded model synthesis только для grouping и responsibility text. Structured output проходит schema/size/scope validation; existence, revision, evidence и trust остаются Core authority.
4. Реализовать normalization: stable component keys, relationship endpoints, boundary membership, evidence sets, coverage diagnostics и explicit omissions. Secret-like literals redacted/rejected from labels and summaries.
5. Реализовать snapshot lifecycle и refresh: full rebuild по explicit request/schema/extractor/massive-change/coverage threshold; otherwise affected evidence → local topology patch → validation → new revision.
6. Реализовать delta и expected-vs-actual review с `Observe`, `WarnOnUnexpected`, `RequireReviewForMaterialChange`. UI coordinates и below-scope implementation detail не создают material drift.
7. Реализовать traversal `Upstream`, `Downstream`, `Route` с bounded results и отдельной provenance; не выдавать route за confirmed impact.
8. Реализовать restart/failure recovery: interrupted refresh не completed, candidate не заменяет last-good, fingerprint перепроверяется, stale/revision recalculated fail-safe, inaccessible root не substituted.

## Fault/recovery matrix

- extractor crash/timeout → bounded diagnostic, last-good snapshot сохранён;
- changed evidence revision → зависимые facts stale, omission не скрывает новый fact;
- unknown model output/invalid identity → Candidate/IdentityUncertain, не Verified;
- root denied/unavailable → evidence denied/stale только для этого root;
- duplicate refresh → idempotent/stale lease outcome;
- delta incompatible → typed incompatibility, без guessed comparison;
- Core restart during refresh → active job не считается completed и безопасно возобновляется или завершается failed.

## Критерии выхода

- [ ] Повторная сборка одного revision детерминирована.
- [ ] Runtime entry point/worker/store/external dependency coverage warnings воспроизводимы.
- [ ] Incremental refresh затрагивает только affected evidence, если policy не требует full rebuild.
- [ ] Last-good snapshot доступен при любой ошибке candidate refresh.
- [ ] Material unexpected drift и expected/missing changes дают typed review result.

## Не входит

IPC/UI, generic graph editor, arbitrary code execution, network discovery и автоматическое применение архитектурных изменений.
