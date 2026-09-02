# План 117.0 — Architecture Snapshot & Drift Review: source-backed topology и reviewable deltas

Статус: предложено по [issue #97](https://github.com/rkfsociety/EvoHime/issues/97). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить в EvoHime Core-owned `ArchitectureSnapshot`: bounded ревизионную модель архитектуры проекта, в которой компоненты, связи и границы имеют stable identity и точное repository evidence. Между совместимыми снимками строится deterministic `ArchitectureDelta`, а Incremental Change Protocol может сравнить ожидаемые и фактические архитектурные изменения.

Snapshot является слоем над Semantic Repository Map: Map остаётся fine-grained индексом symbols/files, а Snapshot — человеческим и машинным архитектурным контрактом. Renderer получает только typed projection и не вычисляет topology, trust или delta.

## Текущее основание и граница

План расширяет существующие Semantic Repository Map (план 86), Artifact Handoff Registry (план 56) и Incremental Change Protocol (план 59). Второй symbol graph, новая capability system или отдельная architecture database не создаются. Авторитетом остаются workspace files, Core validation и revision-pinned artifacts.

Кандидатные поверхности: `crates/evohime-core`, local-storage, authenticated desktop IPC, Electron main/preload/renderer, `docs/architecture.md` и `docs/current-state.md`. Имена модулей, schema revision и IPC tags подтверждаются на evidence freeze по live checkout.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./117-1-architecture-snapshot-drift-review.md)
- [Этап 2 — extraction, runtime и recovery](./117-2-architecture-snapshot-drift-review.md)
- [Этап 3 — IPC, client projection и UI](./117-3-architecture-snapshot-drift-review.md)
- [Этап 4 — verification, release-evidence и закрытие](./117-4-architecture-snapshot-drift-review.md)

## Зависимости

### Блокирующие

- План 86 / Semantic Repository Map: revision-aware map, evidence index и incremental file impact.
- План 56 / Artifact Handoff Registry: durable revision-pinned accepted artifacts.
- План 59 / Incremental Change Protocol: expected-vs-actual change review integration.
- Existing workspace/root grants, Core capability/policy/approval, SQLite backup/migrations, event journal, authenticated IPC и Electron replay/resync.

### Опциональные

- План 75 / Typed Context References и план 82 / Context Mentions — адресные architecture refs и bounded context projection.
- План 53 / Diagnostics & Support Bundle — расширенный redacted export.
- План 80 / Project Instruction Stack — project-owned architecture metadata, если уже доступна соответствующая authority boundary.

## Основной контракт направления

Core вводит versioned `ArchitectureSnapshot`, `ArchitectureComponent`, `ArchitectureRelationship`, `ArchitectureBoundary`, `ArchitectureEvidenceRef`, `ArchitectureCoverageProfile`, `ArchitectureCoverageDiagnostic`, `ArchitectureOmission`, `ArchitectureDelta`, `ExpectedArchitectureDelta` и `ArchitectureChangeReview`.

Каждый material fact содержит root-qualified evidence (`file_ref`, `file_revision`, range/symbol при наличии, evidence kind и content hash). Состояния `Verified`, `Reviewed`, `Candidate`, `Unsupported`, `Stale` различаются явно; модель может предложить Candidate, но не может выдать его за Verified.

Stable identity не зависит только от display name. При неоднозначном сопоставлении delta возвращает `PossibleRename`/`IdentityUncertain`, а не уверенный move. Coverage profile и explicit omissions bounded и revision-pinned: omission не скрывает новый факт после изменения исходного workspace.

Extraction использует только allowlisted deterministic manifests/config/routes/process/schema/message-bus extractors и bounded model synthesis. Extractor не исполняет repository code/scripts. Snapshot freshness: `Fresh`, `PossiblyStale`, `Stale`, `Refreshing`; failed refresh сохраняет last-good accepted snapshot.

## Критерии готовности направления

- [ ] Есть versioned Core-owned snapshot с component/relationship/boundary identity и evidence lineage.
- [ ] Fact states, coverage profile, diagnostics и omission semantics fail-closed и machine-readable.
- [ ] Два compatible snapshot дают deterministic delta/hash с uncertain matches и boundary/evidence/coverage changes.
- [ ] Incremental Change Protocol сравнивает expected и actual delta с `unexpected`, `missing`, `ambiguous` и policy verdict.
- [ ] Incremental refresh инвалидирует affected evidence, а не вызывает полный rebuild без причины.
- [ ] Accepted snapshot/delta хранятся как revision-pinned ProjectArtifacts и корректно восстанавливаются после restart.
- [ ] Electron отображает topology, evidence и Before/Delta/After, оставаясь projection-only.
- [ ] Multi-root grants/identities, redaction, prompt-injection-as-data и security boundaries доказаны тестами.

## Security и non-goals

Repository content остаётся untrusted data; evidence не является capability grant. Core canonicalizes paths и проверяет root scope, secrets не попадают в labels/projections, inaccessible root не подменяется похожим path. Reachability называется route/reachability и не объявляется доказанным blast radius или runtime impact.

Не входят generic drawing editor, полный symbol graph UI, доказательство полной semantic correctness, live production observability, network/cloud discovery, arbitrary extractor plugins, автоматический запрет любого drift и отдельная architecture database.

## Связанный issue

- [#97 Architecture Snapshot & Drift Review](https://github.com/rkfsociety/EvoHime/issues/97)
