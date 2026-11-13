# План 195.2 — Core registry integration и workflow composition

Статус: active implementation contract. Этап следует после 195.1.

## Scope и изменяемые контракты
1. Интегрировать descriptors в canonical existing capability/tool owners. Если capability catalog уже хранит нужные fields, расширить его совместимо вместо второго registry.
2. Admission выполняется Core: descriptor/schema/trust/dependencies, adapter availability, caller grant, approval, resource budget и sandbox проверяются до запуска.
3. В workflow runtime добавить references на pinned capability descriptor versions и typed node contracts. Сохранять deterministic graph hash и execution/layout hash semantics.
4. Node output передавать через существующий ArtifactStore с provenance; downstream node получает только declared typed refs, не arbitrary paths или полномочия предыдущего node.
5. Retry policy задавать по operation semantics: read-only/idempotent, retry-safe mutation с exact idempotency key, либо non-retryable/unknown outcome. Не повторять ambiguous effects.
6. Workflow composition сохраняет approval boundaries: approval одного node не становится grant для других nodes или будущих runs.
7. Активные workflow runs используют snapshot pinned versions; обновления применяются к новым runs после existing transaction/health gates.
8. Migration только при необходимости; предпочтительно additive metadata и existing stores.

## Зависимости
### Блокирующие
195.1; existing Capability Registry/ToolRegistry, workflow runtime, ArtifactStore, PermissionEngine, approval, sandbox and updater contracts.
### Опциональные
План 193 team workflows; план 194 job origin/recipe display.

## Recovery и rollback
Invalid/missing adapter не стартует node. Workflow сохраняет typed unavailable/incompatible outcome и продолжает только если graph policy явно разрешает optional branch. Restart использует pinned hashes и existing workflow recovery. Rollback отключает новые node descriptors, не меняя historical runs/artifacts.

## Verification
- Integration tests for descriptor-to-adapter resolution and policy admission.
- Workflow graph tests for typed input/output, dependency ordering, hashes and deterministic execution.
- Artifact provenance and size-bound tests.
- Approval isolation between nodes/runs.
- Failure injection: missing adapter, revoked trust, update during run, stale schema, retry ambiguity and crash recovery.
- Verify no new scheduler, updater or executable plugin loader.

## Release evidence
Descriptor/schema hashes, adapter resolution outcomes, workflow graph hashes, node provenance refs, approval/retry outcomes, migration evidence and CI links.

## Критерии выхода
- [ ] Workflow runtime remains sole workflow executor.
- [ ] Capability resolution cannot bypass permissions or sandbox.
- [ ] Nodes exchange bounded typed artifacts, not implicit grants.
- [ ] Pinned runs are stable across extension updates.
