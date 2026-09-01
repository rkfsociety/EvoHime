# План 98.2 — Durable Remote Task Bridge: submit/status/cancel protocol для долгих tool и MCP операций: runtime-интеграция и recovery

Статус: этап 2 для [плана 98.0](./98-0-durable-remote-task-bridge.md); после [плана 98.1](./98-1-durable-remote-task-bridge.md).

## Цель

Провести «Durable Remote Task Bridge: submit/status/cancel protocol для долгих tool и MCP операций» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 98.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Composable Termination Conditions v1 — зависимость из канонических документов.
- План 77.0 — зависимость из обзора.
- План 43.0 — зависимость из обзора.
- План 45.0 — зависимость из обзора.
- План 54.0 — зависимость из обзора.
- План 93.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/durable_remote_task_bridge.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `DurableRemoteTaskBridgeService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/durable_remote_task_bridge_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть versioned RemoteTaskToolset/RemoteTaskRecord contracts. → провести через typed outcome, timeout, cancellation и idempotency.
- `C02` — Submit/status/cancel lifecycle Core-owned и durable. → журналировать переходы и восстановление через replay/reconciliation.
- `C03` — Pending tasks переживают restart. → журналировать переходы и восстановление через replay/reconciliation.
- `C04` — Polling bounded, leased и backoff-aware. → провести через typed outcome, timeout, cancellation и idempotency.
- `C06` — Results сохраняются как structured data/artifact refs. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C07` — MCP и Integration Provider могут использовать один bridge. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.

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
