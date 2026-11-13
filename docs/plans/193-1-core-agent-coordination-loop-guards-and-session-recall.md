# План 193.1 — contracts и deterministic guard/query core

Статус: active implementation contract. Этап следует после принятия overview 193.0.

## Scope и изменяемые контракты
1. Провести code-level audit действующих ToolRegistry, agent loop, TaskCoordinator, EventJournal, execution ledger, assignment/handoff, TeamSession, memory recall и conversation history. Зафиксировать точные owners и revisions; не выводить отсутствие функции только из отсутствия отдельного UI.
2. Определить canonical loop signature: task/run/agent scope, tool id, canonical input hash, relevant state/evidence revision, outcome class и bounded time window. Секреты и raw args в signature/report не включать.
3. Определить typed progress signals: verified state delta, new evidence ref, accepted child result, successful state transition. Повторение текста/heartbeat не считается прогрессом.
4. Ввести versioned guard policy с отдельными лимитами repeated-call, repeated-error, oscillation, no-progress window и total step/tool budget. Defaults conservative; per-task overrides только Core-owned и capped.
5. Определить typed outcomes: continue, strategy_change_required, delegate_candidate, paused_for_review, stopped_loop, budget_exhausted, unknown_progress. Outcome сам по себе не выполняет tool call.
6. Определить session-query/v1: query scope, allowed sources, time range, max results/bytes, ranking provenance, trust/authority metadata и partial/unavailable semantics. Использовать существующие stores; миграция только при доказанном query bottleneck.
7. Определить coordination envelope: parent task, child assignment ref, pinned team protocol hash, capability intersection, deadline, budget, expected artifact/evidence refs и deterministic merge requirements.

## Зависимости
### Блокирующие
ToolRegistry/PermissionEngine; EventJournal/execution ledger; Agent Organization, TeamSession, collaboration bus и child handoff; Context Namespace, Memory Views & Adaptive Recall и conversation log.
### Опциональные
План 184 trace evaluation; план 186 typed decision fast path — guard не зависит от LLM selection.

## Recovery и rollback
Unknown/invalid policy не разрешает automatic continue/delegate: typed guard_unavailable или safe pause. Query timeout/partial index даёт partial, не пустой успешный ответ. Rollback снимает новые contracts, не удаляя существующие events, assignments или memory records.

## Verification
- Canonicalization одинаково обрабатывает эквивалентные JSON inputs и различает tool ids/scopes.
- Raw arguments, credentials и transcript bodies не попадают в signature.
- Progress требует durable/verified evidence ref или monotonic state revision.
- Fixtures покрывают repeated call/error, A-B-A oscillation, transient retry, legitimate polling и idempotent reads.
- Query tests проверяют scope isolation, authority/trust labels, limits, stable ordering, partial/unavailable и redaction.
- Compatibility tests сохраняют ToolManifest, approval, TeamSession и Context Namespace semantics.

## Release evidence
Contract/policy versions, canonical signature fixture hashes, query schema hash, bounded reason codes и unit/schema test results. Не включать реальные пользовательские запросы, raw tool inputs или memory bodies.

## Критерии выхода
- [ ] Canonical loop/progress/query contracts versioned и documented.
- [ ] Guard outcomes не содержат скрытого разрешения на tool execution.
- [ ] Query не создаёт новый persistence owner.
- [ ] False-positive и partial-data cases имеют deterministic semantics.
- [ ] Внутренние ссылки и git diff --check проходят.
