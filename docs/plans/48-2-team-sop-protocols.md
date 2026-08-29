# План 48.2 — Team SOP Protocols: versioned multi-agent playbooks и формальные handoff правила: runtime-интеграция и recovery

Статус: этап 2 для [плана 48.0](./48-0-team-sop-protocols.md); после [плана 48.1](./48-1-team-sop-protocols.md).

## Цель

Провести «Team SOP Protocols: versioned multi-agent playbooks и формальные handoff правила» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 48.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 46.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/team_sop_protocols.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `TeamSopProtocolsService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/team_sop_protocols_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Participants ссылаются на versioned Agent Role Profiles. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C03` — Phases/handoffs/exit criteria формализованы. → провести через typed outcome, timeout, cancellation и idempotency.
- `C04` — Есть bounded review/revise loop. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — TeamProtocol исполняется поверх существующего workflow/child runtime. → провести через typed outcome, timeout, cancellation и idempotency.
- `C06` — TeamSession имеет immutable protocol snapshot. → журналировать переходы и восстановление через replay/reconciliation.

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
