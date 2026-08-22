# 09-1 — Capability snapshot и typed policy contract

## Цель

Зафиксировать versioned набор прав и лимитов, который Core создаёт для каждого
run и связывает с каждым action. Snapshot — immutable input для planning,
approval и execution; он не является grant, который можно расширить из UI.

## Зависимости

### Блокирующие

- [09-0](09-0-policy-and-capabilities.md);
- план 08 [`08-1`](08-1-ledger-contract.md) и
  [`08-2`](08-2-ledger-storage-and-recovery.md): action/run identity, durable
  action linkage, bounded retention и replayable terminal outcome;
- текущие permission, manifest, receipt и authenticated IPC contracts.

### Опциональные

- будущие adapter-specific scopes: до появления adapter значение отсутствует,
  а попытка вызвать его возвращает `unavailable`, не `allow`;
- catalog metadata из [07-2](07-2-toolkit-catalog-lifecycle.md): hash
  установленного manifest остаётся достаточным fallback для policy binding.

## Что уже есть в коде

- typed permissions, scoped path grants и hard policy rules
  (`crates/permissions/src/lib.rs`, `policy.rs`);
- `RunPolicy`, manifest hashes и canonical exact-call hashes;
- durable action/approval binding (`receipt_actions`,
  `receipt_approval_intents` в `crates/evohime-receipts/src/runtime.rs`), к
  которому snapshot linkage добавляется additive-колонками;
- domain-separated canonical JSON и hash contract из receipts, который
  переиспользуется без ввода второго canonicalizer.

Нет единого versioned snapshot, его canonical hash не сохраняется в approval
intent и action, parent/child subset проверяется только в отдельных контурах,
а persisted `policy_decision` ограничен значениями
`allow`/`deny`/`approval_required`.

## Контракт

Ввести `CapabilitySnapshotV1` с bounded полями:

- `snapshot_id`, `run_id`, `session_id`, `task_id` и optional
  `parent_snapshot_hash`;
- `policy_id`, `policy_version`, `policy_hash` и установленный
  `manifest_hash`;
- canonical workspace anchors и operation scopes; абсолютный путь может быть
  только нормализованным представлением anchor, а не новым grant;
- permissions и tool identities с operation type (`read`, `write`, `execute`,
  `network`, `memory` и т. п.);
- network route policy и adapter scopes как opaque, versioned references;
- opaque `secret_ref`/purpose pairs без secret values;
- timeout, input/output size, concurrency, tool-call, token и cost budgets.

`run_id`, `session_id` и `task_id` — те же идентичности, что и в ledger
плана 08: snapshot не заводит параллельное пространство имён.

Snapshot canonicalize-ится существующим domain-separated canonical JSON
контрактом и получает `snapshot_hash`. Неопознанная версия, дубликат identity,
пустой обязательный scope, превышенный limit или невалидный hash дают
fail-closed `policy_error`.

Run snapshot задаёт верхнюю границу. Action binding дополнительно фиксирует
`action_id`, canonical call hash и effective snapshot hash. Approval intent и
receipt/action хранят этот hash и `policy_version`; изменение любого из них
делает approval stale, даже если tool и path остались прежними.

Parent → child разрешает только subset:

- permissions/tool identities — подмножество;
- workspace/network/adapter scopes — подмножество;
- timeout, sizes, concurrency и budgets — не больше остатка parent;
- secret refs — только уже выданные refs с тем же purpose.

Нельзя унаследовать capability через свободный текст, prompt, renderer или
модельный risk signal. Child с отсутствующим parent snapshot не запускается.

## Persistence и IPC

- расширить action/receipt/approval linkage плана 08 полями snapshot id/hash,
  policy version и bounded redacted snapshot payload;
- payload не содержит secret values, raw provider credentials, произвольный
  prompt или необрезанный tool input; retention следует storage/ledger policy;
- добавить additive redacted capability summary для UI: identities, modes,
  scopes summary, limits, hash и expiry, но не path secrets, token values или
  authority-changing commands;
- не создавать отдельную базу или независимый approval store.

Typed policy decision имеет stable `outcome` и bounded `reason_code`:
`allowed`, `approval_required`, `denied`, `unavailable`, `expired`,
`cancelled`, `policy_error`. `denied` и `policy_error` не retry-ятся сменой
входа в том же action; `unavailable` может быть повторен только после
изменения availability, а `expired` требует нового snapshot/action.

Расширение словаря — additive: существующие persisted значения
`allow`/`deny`/`approval_required` сохраняют смысл и не переопределяются, новые
outcomes добавляются миграцией вместе с детерминированным mapping старых строк.
Offline verifier и export должны читать оба поколения записей.

## Проверки

- serde/canonical round-trip, domain separation, size limits и fixed hash
  fixtures;
- snapshot tamper, unknown version, missing capability, duplicate identity и
  stale hash — без dispatch;
- parent/child subset fixtures для каждого поля и запрет budget/scope
  escalation;
- approval/receipt action связываются с task/session/run/action, call hash,
  policy version и snapshot hash;
- snapshot/IPC/export/log fixtures доказывают отсутствие secret values и
  необрезанного input;
- deterministic mapping всех typed outcomes и retryability, включая чтение
  записей, созданных до миграции словаря.

## Готово, когда

Каждая операция получает неизменяемый bounded snapshot, его hash неизменно
связан с canonical action и approval/receipt, а child не может получить права
шире parent. Snapshot, который невозможно безопасно восстановить или
проверить, блокирует effect с typed `policy_error`.
