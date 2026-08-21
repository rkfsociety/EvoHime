# 09-1 — Capability snapshot и typed policy contract

## Цель

Зафиксировать versioned набор прав и лимитов, который создаётся для каждого
run и используется одинаково на planning, approval и execution границах.

## Изменения

1. Ввести `CapabilitySnapshotV1` с run/session identity, policy version/hash,
   tool permissions, workspace anchors, network routes, browser sessions,
   secret references, timeout, size, concurrency и provider budgets.
2. Привязать snapshot к immutable canonical call и execution ledger action.
3. Разделить hard policy decision, approval state и advisory risk signal;
   advisory model risk не может разрешить запрещённую операцию.
4. Описать typed outcomes `allowed`, `approval_required`, `denied`,
   `unavailable`, `expired`, `cancelled` и `policy_error`.
5. Запретить изменение permission, scope, input, tool identity или snapshot
   hash после создания approval intent.

## Интерфейсы

- Rust typed snapshot/decision types и canonical hash;
- bounded persistence/linkage к run и action;
- additive IPC projection только для redacted capability summary;
- совместимость с существующими signed receipts и approval IDs.

## Проверки

- serde/canonical round-trip и hash fixtures;
- parent/child snapshot subset checks;
- negative tests на permission/scope/input mutation;
- unknown version, missing capability и stale snapshot typed errors.

## Готово, когда

Каждая операция получает неизменяемый capability snapshot, а любой результат
можно связать с конкретной версией policy и run.
