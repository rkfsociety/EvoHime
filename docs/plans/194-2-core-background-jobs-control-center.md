# План 194.2 — Core integration, recovery и bounded controls

Статус: active implementation contract. Этап следует после 194.1.

## Scope и изменяемые контракты
1. Подключить aggregation/read model к существующему background execution owner и workflow/automation/agent sources. Не переносить lifecycle logic в новый facade; facade только нормализует bounded metadata.
2. Добавить typed action commands только для подтверждённых операций: cancel/stop, open evidence и безопасный refresh. Retry/pause/resume оставить unavailable, если owner не гарантирует semantics.
3. Каждая mutation проверяет owner, current revision, lifecycle precondition, permission/approval и idempotency key непосредственно в Core.
4. Cancellation распространяется через существующие cancellation tokens, child tasks и supervisor Job Object contracts. UI отображает request accepted отдельно от terminal cancelled state.
5. Restart reconciliation возвращает durable state из plan 132. Unknown outcome и adapter unavailable не преобразуются в retry или success.
6. События прогресса агрегировать из фактических этапов/attempts; если источник не публикует progress, показывать stage/status без выдуманного процента.
7. Учитывать resource pressure и task leases при сортировке/подсказках, но не переопределять admission и lease owners.

## Зависимости
### Блокирующие
194.1; plan 132 runtime, workflow/automation owners, cancellation, task leases, supervisor lifecycle и resource admission.
### Опциональные
План 193 assignment diagnostics; план 195 workflow recipe metadata.

## Recovery и rollback
Duplicate cancel is idempotent for the same job revision. Stale action returns typed revision_conflict; terminal state cannot be reopened by UI. If adapter is absent, command returns runtime_adapter_unavailable. Rollback removes new action routes; original owner remains operational.

## Verification
- Integration tests for supported job types and status transitions.
- Cancel race tests: queued/running/terminal/restarted jobs.
- Duplicate/stale action tests with idempotency and revision fencing.
- Restart recovery tests for waits, wakeups, attempts and unknown effects.
- Ensure no progress percentage is emitted without a measured denominator.
- Ensure action calls cannot bypass approval or resource admission.

## Release evidence
Redacted job ids, revisions, transition classes, cancellation/recovery outcomes, action command versions and CI evidence. No payloads or logs.

## Критерии выхода
- [ ] All lifecycle changes route through the original owner.
- [ ] Cancel/stop result is distinct from request acceptance.
- [ ] Unknown outcome never receives blind retry.
- [ ] Progress is evidence-based and bounded.
