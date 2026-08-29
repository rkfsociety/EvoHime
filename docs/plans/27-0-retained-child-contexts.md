# План 27.0 — Retained Child Contexts и mailbox

Статус: предложено по [issue #8](https://github.com/rkfsociety/EvoHime/issues/8).
Issue закрыт как источник требований; закрытие issue не считается доказательством
реализации. План расширяет существующий Core-owned child workflow, но не заменяет
typed request/report, grants, provenance, leases или deterministic fan-in.

## Цель и граница

После завершённого child run Core может по явному parent-действию сохранить
адресуемый контекст и выдать тому же child bounded follow-up. `TypedChildReport`
остаётся результатом конкретной revision/run; retained lifecycle и mailbox —
отдельный communication layer. Старый report никогда не считается актуальным
после изменения workspace или выбранных artifacts.

MVP ограничен одним Core process, одной parent family и локальным SQLite. Retained
context состоит только из durable metadata и разрешённых refs: скрытый in-memory
transcript не сохраняется и не является recovery source. `active_session_id`,
если он нужен runtime, ссылается на Core-owned session и не выдаётся клиенту.
Sender identity выводится из authenticated runtime context, а не принимается из payload.
Поля, лимиты и имена протокола ниже являются контрактом для ревью; numeric IPC
tags назначаются только после проверки live proto и фиксируются в evidence.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./27-1-retained-child-contexts.md)
- [Этап 2 — runtime-интеграция и recovery](./27-2-retained-child-contexts.md)
- [Этап 3 — IPC, client projection и UI](./27-3-retained-child-contexts.md)
- [Этап 4 — verification, release-evidence и закрытие](./27-4-retained-child-contexts.md)

## Зависимости

### Блокирующие

- live child surfaces: `crates/evohime-core/src/child_contracts.rs`,
  `child_runtime.rs`, `child_workflow.rs`, `child_roles.rs`;
- `crates/evohime-local-storage/src/child_store.rs`, `LocalDatabase` migration
  ladder, `ArtifactStore` allowlist/read policy и parent sequence;
- действующие capability/grant, provenance, lease, cancellation, audit и
  authenticated IPC boundaries;
- канонические контракты TaskCheckpoint из `docs/architecture.md` и
  `docs/current-state.md` (план 23 удалён после реализации);
- текущая live storage schema `v36`, проверенная перед реализацией; следующая
  additive revision (ожидаемо `v37`) не фиксируется без подтверждения кода.

### Опциональные

- реализованный контракт плана 26 (Continuation Policy): retained child обязан
  работать с explicit follow-up без него; автоматическая доставка допускается
  только через уже реализованный policy и при typed `unavailable`/`blocked`;
- Goal linkage: при отсутствии Goal retained registry остаётся parent-scoped и
  не получает ложную связь с Goal.

## Normative contract

`RetainedChildV1` parent-scoped и содержит bounded `child_id`, `parent_id`,
`family_root_id`, `role`, optional `stable_name`, lifecycle status, текущую
revision, optional active session, current grant snapshot hash, context scope
hash, workspace state ref, last report ref, retained-until, created/last-active
timestamps и monotonic `registry_version`. Комбинация `(parent_id, child_id)`
уникальна; чужой parent не может читать, менять или удалять запись.

Lifecycle `Active → IdleRetained → QueuedFollowUp → RunningFollowUp →
IdleRetained` не смешивается с run states. Terminal run может перейти в
`IdleRetained` только отдельной idempotent `retain` mutation. `Expired`,
`Deleted` и `Invalidated` terminal для retained registry; их mailbox entries
становятся недоставляемыми typed outcomes.

`ChildFollowUpRequestV1` содержит idempotency key, parent/child/family identity,
parent sequence, expected child revision, bounded instruction, selected context
refs, requested grant subset, bounded budget, `follow_up|steer|auto` mode и
correlation id. `follow_up` — default и только после завершения текущего run;
`steer` запрещён, пока role/runtime capability явно его не разрешает; `auto`
разрешает dispatch idle child или durable queue busy child, но не создаёт
неявную capability/approval.

Mailbox entry содержит message id, Core-derived sender/receiver, family root,
mode/kind, correlation id, parent sequence, payload ref или bounded inline
payload, sensitivity, delivery state и delivered timestamp. Delivery model —
at-least-once transport с durable idempotency/deduplication; exactly-once
внешнего эффекта не обещается. `Pending`, `Dispatched`, `Delivered`,
`Rejected`, `Blocked`, `Unknown` и `Expired` должны быть различимы.

Предлагаемые defaults, подлежащие подтверждению на шаге 0: inline payload 32 KiB,
32 pending messages на child, 64 follow-ups на retained child, 120 requests/hour
на parent, 16 retained children на parent, 32 на Goal, TTL 24 hours и artifact
offload только через существующую policy. Overflow не теряет запись: возвращает
typed `limit_exceeded`/`blocked` и сохраняет audit outcome.

## Revalidation, freshness и recovery

Каждый retain, read, delete и follow-up заново проверяет authenticated parent
chain, child identity, expected revision, текущие grants/role maximum, policy,
context allowlist, artifact existence/sensitivity, budget, provenance, payload
limits и TTL. Старый grant snapshot — только provenance. Sender/receiver из
payload игнорируются.

Follow-up сравнивает relevant workspace/context snapshot и hashes выбранных
refs с последним report. Missing artifact, changed grant/policy или hash drift
даёт явный `stale`/`invalidated` outcome; новый delta/evidence передаётся явно.
Никакого blind reuse старого transcript.

Reserve sequence, mailbox state и registry version должны изменяться одной
SQLite-транзакцией. После restart registry и pending delivery reconciled по
sequence/status; uncertain delivery не повторяется без reconciliation; lease и
boot id проверяются для running child; expired не rehydrate-ится автоматически.
Deleted child инвалидирует retained refs и mailbox destinations.

Child-to-parent `Progress`, `Question`, `Blocked`, `EvidenceReady` и
`FollowUpResult` — bounded typed messages и не меняют formal report/fan-in.

## UI и IPC

Core добавляет additive typed commands `ListRetainedChildren`, `GetRetainedChild`,
`RetainChild`, `SendChildFollowUp` и `DeleteRetainedChild`, а также bounded typed
projection/events для registry/mailbox. Numeric tags, точный event shape,
generated Rust/C#/TypeScript types и compatibility behavior фиксируются после
шага 0; старые clients продолжают работать и не получают новые capabilities.
OperationsPanel показывает только metadata: name/role, lifecycle, revision,
last activity, pending count, TTL, stale/invalidated reason, Goal/workflow refs
и typed actions. Raw prompt, transcript, hidden reasoning, credentials,
Sensitive/Secret payload и absolute paths не выдаются renderer или compatibility
client.

## Acceptance всей темы

- registry и follow-up durable, parent-scoped и versioned;
- mailbox bounded, idempotent и с различимыми delivery/unknown outcomes;
- grants, context, provenance, revision, freshness и TTL проверяются на каждом
  effect/read/mutation;
- terminal child report/fan-in остаётся отдельным контрактом;
- restart, queue, expiry, deletion, invalidation и uncertain delivery recoverable;
- sibling escape, secret payload и raw transcript не попадают в projections;
- проходят Core/storage/recovery/Rust IPC/Electron/C# compatibility/security
  tests и `git diff --check`.

## Не входит

Свободный P2P chat, внешняя сеть агентов, бессрочное хранение, полный parent
transcript, ослабление grants, самостоятельный renderer authority, новый
transport или замена workflow fan-in.
