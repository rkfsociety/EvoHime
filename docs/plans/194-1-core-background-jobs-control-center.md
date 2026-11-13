# План 194.1 — projection и lifecycle action contract

Статус: active implementation contract. Этап следует после overview 194.0.

## Scope и изменяемые контракты
1. Сопоставить существующие fields command 262/event 107 с UX needs: owner, origin, lifecycle state, timestamps, next wake, queue/wait reason, attempt summary, dependency refs, progress evidence, artifact refs и typed error.
2. Для отсутствующих полей определить additive BackgroundJobSummaryV1. Projection агрегирует references, а не копирует job payload.
3. Определить pagination/cursor contract, stable ordering, maximum page size/bytes, retention boundary и stale cursor behavior.
4. Инвентаризировать реально поддерживаемые Core actions. Для каждой определить authorization owner, preconditions, revision/idempotency token, terminal states и typed unavailable response.
5. Не объявлять pause/resume/retry, если underlying owner не реализует корректную семантику. UI control может отсутствовать в V1.
6. Определить status taxonomy и mapping без потери разницы между WaitingForResources, WaitingForApproval, UnknownOutcome, runtime_adapter_unavailable и terminal failure.
7. Зафиксировать безопасные summaries/errors без prompt, raw payload, credentials и absolute paths.

## Зависимости
### Блокирующие
Plan 132 contract/schema and command 262/event 107; workflow/automation/agent/goal/human-work/remote task lifecycle owners; task lease/fencing, approval and ArtifactStore provenance.
### Опциональные
План 193 assignment metadata; existing UI task timeline components.

## Recovery и rollback
Projection version additive. Unknown fields ignored by older clients; unknown status maps to explicit unknown, not completed. Expired cursor returns typed stale snapshot and client reloads first page. No persistence migration unless indexed read bottleneck is measured and documented.

## Verification
Schema compatibility, stable cursor ordering, owner mapping, pagination, redaction, stale revision, unavailable adapter and terminal-state tests. Verify no UI action mutates lifecycle without Core-owned precondition.

## Release evidence
Projection schema/hash, field ownership map, status mapping, page/byte bounds, action precondition matrix and focused contract test results.

## Критерии выхода
- [ ] Projection fields are sourced from authoritative owners.
- [ ] Unsupported lifecycle actions are explicitly unavailable.
- [ ] Unknown/stale/partial states are never presented as success.
- [ ] No duplicate storage or scheduler is introduced.
