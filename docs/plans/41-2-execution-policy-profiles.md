# План 41.2 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation: runtime-интеграция и recovery

Статус: этап 2 для [плана 41.0](./41-0-execution-policy-profiles.md); после [плана 41.1](./41-1-execution-policy-profiles.md).

## Цель

Провести «Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 41.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/execution_policy_profiles.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `ExecutionPolicyProfilesService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/execution_policy_profiles_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть versioned ExecutionPolicyProfile. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C02` — Shell/process tools запускаются только через resolved profile. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C04` — Есть process-tree lifecycle и bounded output/timeouts. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Workspace/network permissions представлены явно. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C08` — Windows-first restricted backend исследован и реализован хотя бы для одного практичного режима. → провести через typed outcome, timeout, cancellation и idempotency.

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
