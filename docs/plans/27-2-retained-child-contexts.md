# План 27.2 — Retained Child Contexts и mailbox: runtime-интеграция и recovery

Статус: самостоятельный этап 2 для [плана 27.0](./27-0-retained-child-contexts.md); начинается после [плана 27.1](./27-1-retained-child-contexts.md).

## Зависимости

### Блокирующие

- contract/storage из [27.1](./27-1-retained-child-contexts.md);
- `child_workflow.rs` coordinator, `child_runtime.rs` run states, existing lease,
  cancellation, artifact/context policy, audit/event journal и Core IPC boundary.

### Опциональные

- Continuation Policy: `auto` без неё выдаёт typed `unavailable`, не dispatch;
- Goal linkage: отсутствующий Goal не блокирует parent-scoped flow.

## Реализация по шагам

0. Проверить contract hash/schema revision и подготовить immutable
   `policy/grant/context` snapshot для каждого run. Составить matrix terminal
   child → retain, idle → follow-up, busy → queue, expired/deleted/invalidated
   → typed rejection.
1. В Core handler реализовать idempotent retain/get/list/delete и follow-up
   state machine. Authorization повторяется непосредственно перед effect;
   optimistic `registry_version` и expected revision защищают stale actions.
   Terminal child run не переводится обратно в run state: follow-up создаёт
   отдельную revision/run, связанную с retained registry.
2. Реализовать parent-mediated mailbox routing. Receiver/sender берутся из
   authenticated coordinator; sibling route запрещён. `follow_up` dispatchится
   только idle, `auto` queue-ит busy child, `steer` fail-closed без explicit
   role/runtime allowance. Queue admission, rate и TTL атомарны с sequence.
3. Перед dispatch повторить grant subset, role maximum, policy/approval,
   context/artifact allowlist, provenance, budget и freshness checks. При drift
   сохранить `invalidated`/`stale` outcome и не запускать child.
4. Зафиксировать at-least-once delivery: dispatch marker до передачи, durable
   dedup по idempotency/correlation, terminal delivery states и no blind retry
   после `Unknown`. Lease/boot id reconciliation после restart не повторяет
   внешний effect автоматически.
5. Добавить fault injection до/после marker, crash/restart с pending queue,
   stale lease/version, duplicate delivery, policy/grant narrowing, missing
   artifact, queue overflow и corrupted state. Подготовить metadata-only
   projection fixture для 27.3.

## Артефакты и критерии выхода

- Core handler/state machine с отдельными retained/run lifecycles;
- durable delivery/recovery transitions и documented unknown outcome;
- bounded queue, cancellation/timeout/lease/retry/partial-failure matrix;
- reproducible integration/fault-injection tests и stable command/event inventory.

Этап считается готовым только если основной idle follow-up даёт typed result,
busy `follow_up` durable queue-ится, duplicate не создаёт второй run, stale,
denied, limit, expired, invalidated, cancelled, restart и unknown cases имеют
различимые outcomes, а formal report/fan-in не меняется.

## Не входит

Client authority, direct storage access из UI, новый transport, provider/runtime
вне существующих boundaries и неявный network fallback.
