# План 27.0 — Retained Child Contexts и mailbox

Статус: предложено по [issue #8](https://github.com/rkfsociety/EvoHime/issues/8).
План расширяет существующий child workflow, но не заменяет typed request/report,
leases, grants, provenance, fan-in или формальные run states.

## Цель и граница

После завершения специализированного child сохранить адресуемый context и
разрешить родителю отправить ограниченный follow-up тому же child. `TypedChildReport`
остаётся результатом конкретной revision/run; retained child — отдельный
lifecycle/communication layer. Старый report не считается актуальным после
изменения workspace.

## Зависимости

### Блокирующие

- существующие `child_contracts.rs`, `child_workflow.rs`, `child_runtime.rs`,
  typed grants/provenance, monotonic lease recovery и deterministic fan-in;
- существующий child SQLite store/checkpoint и parent sequence;
- ArtifactStore/context allowlist с sensitivity/read policy;
- план 23 TaskCheckpoint для durable freshness/recovery linkage;
- authenticated IPC и OperationsPanel typed child projection.

### Опциональные

- план 25 Persistent Goals для Goal linkage; retained child должен также работать
  под обычным parent workflow;
- план 26 Continuation Policy для auto delivery; базовый follow-up остаётся
  explicit и bounded;
- план 28 Analysis Kernel для read-only object refs.

## Registry и lifecycle

Ввести parent-scoped `RetainedChildV1`: `child_id`, `parent_id`, `role`, optional
stable name, `status`, current revision, active session, grant snapshot hash,
context scope hash, workspace state ref, last report ref, retention TTL и
timestamps. Lifecycle `Active`, `IdleRetained`, `QueuedFollowUp`,
`RunningFollowUp`, `Expired`, `Deleted`, `Invalidated` не смешивать с run state.

Follow-up request содержит id, parent/child identity, parent sequence, expected
child revision, bounded instruction, selected context refs, requested grants,
budget, delivery mode и correlation id. Delivery modes: safe `follow_up`, strict
`steer` only when role/runtime explicitly allows it, and `auto` that dispatches
idle child or queues when busy.

Добавить Core-owned mailbox entries with sender/receiver/family root, mode/kind,
correlation/sequence, bounded inline or artifact payload, sensitivity, status and
delivery timestamp. Parent-mediated routing is required; free sibling addressing
is disabled in MVP.

## Revalidation, freshness и recovery

Каждый follow-up заново проверяет parent chain, child identity, revision,
current parent grants, role maximum, retained constraints, context allowlist,
budget, provenance, payload limits и TTL. Старый grant snapshot — только
provenance, не вечное разрешение.

Перед dispatch Core сравнивает workspace/context/artifact snapshot. Delta/new
evidence добавляется явно; serious drift или missing artifact переводит child в
`Invalidated` вместо слеплого использования старого контекста.

Queue limits включают payload size, pending count, follow-up count, rate,
retained children per parent/Goal, TTL и artifact policy. Overflow даёт typed
error/blocked state, не silent drop. После restart registry и pending delivery
сверяются по durable sequence/status; uncertain delivery не дублируется
вслепую, running child проверяется по lease/boot id, expired child не
rehydrate-ится автоматически.

Child-to-parent progress/question/blocked/evidence messages не заменяют formal
report и не могут изменить fan-in decision. Deleting a child invalidates
mailbox destinations and retained refs.

## UI и IPC

В OperationsPanel показать stable name/role, retained state, revision, last
activity, pending count, TTL, stale/invalidated warning и связь с Goal/workflow.
Действия retain/send follow-up/delete требуют Core idempotency и policy check.
Raw child transcript по умолчанию не выдаётся; renderer получает metadata-only
projection.

## Этапы реализации

1. Зафиксировать lifecycle transitions, follow-up/mailbox contracts, limits и
   parent-mediated routing policy.
2. Добавить registry/mailbox schema с per-parent sequence, idempotency и retention.
3. Подключить retain, follow-up dispatch, busy queue, grant/context revalidation
   и workspace freshness.
4. Реализовать restart reconciliation, expiry/delete/invalidation и typed IPC/UI.
5. Добавить integration/security tests для delivery, isolation, stale refs и
   redaction; обновить canonical architecture после реализации.

## Критерии готовности

- parent-scoped registry и typed follow-up contract durable;
- mailbox доставляет/очередит bounded messages с duplicate protection;
- grants, context, provenance, revision и freshness проверяются на каждом run;
- formal child report/fan-in остаётся отдельным контрактом;
- restart, pending delivery, expiry, deletion и uncertain outcome recoverable;
- sibling escape, secret payload и raw transcript не попадают в UI;
- limits/overflow дают явный typed outcome;
- проходят child, storage, recovery, IPC и security tests.

## Не входит

Свободный P2P chat, независимая сеть агентов, бессрочное хранение, полный parent
transcript, ослабление grants или замена workflow fan-in.
