# План 33.2 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures: runtime-интеграция и recovery

Статус: этап 2 для [плана 33.0](./33-0-integration-provider-sdk.md); после [плана 33.1](./33-1-integration-provider-sdk.md).

## Цель

Провести «Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 33.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure,
   cancellation и unknown outcome; после restart только replay/reconciliation,
   без blind retry. Реальный secret выдаётся только конкретному зарегистрированному
   Core-owned adapter на время операции и не попадает в generic tool/workflow
   context.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/integration_provider_sdk.rs` плюс
  фактическая точка регистрации/dispatch в Core, подтверждённая на этапе
  evidence freeze; provider operation path должен выполнять `validate → policy
  → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/integration_provider_sdk_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — credential lifecycle → выполнить typed connect/verify/refresh/revoke
  transitions через resolver boundary, с expired/invalid/reconnect outcomes.
- `C03` — scopes/risk → повторно пересечь action metadata с actual credential
  grants и Core approval policy перед effect.
- `C05` — dependency report → перед revoke/remove построить deterministic report;
  dependent definitions становятся unresolved/disabled, не удаляются молча.
- `C06` — webhook capability → только adapter capability declaration из stage 1;
  фактический ingress/enqueue остаётся stage 34, без trigger runtime здесь.
- `C07` — fixtures → прогнать built-in provider/action fixtures through the
  same bounded adapter path, without real credentials.
- `C08` — secret boundary → fault tests доказывают отсутствие secret в event,
  trace, artifact, retry context и client projection.

### Recovery contract

- Durable transitions восстанавливаются replay/reconciliation; transient work после restart получает typed `unknown`/`unavailable`, а не повтор side effect.
- Fault injection должна доказать отсутствие duplicate effect, потерю approval, обход policy или расширение capability set.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
