# План 175.0 — Memory Ingestion Integrity

Статус: active implementation contract. Исторический источник постановки:
issue #153; функционал этим документом не считается реализованным.

## Цель

Сделать автоматическое извлечение памяти Core-owned effectful subsystem:
reentrancy-safe, idempotent, provenance-preserving, monotonic относительно
source freshness и восстанавливаемой после crash/cancel/restart.

Текущий checkout уже имеет bounded `memory_extraction` policy/validation,
отдельный ambient entry point, `memory_entries`, provenance и candidate states.
План закрывает orchestration/concurrency boundary вокруг этих владельцев, не
заменяя memory governance, retrieval или model gateway.

## Архитектурная граница

~~~text
turn/ambient event -> Core extraction gate -> durable candidate/finalization
-> atomic freshness/CAS publish -> existing memory governance/retrieval
~~~

Основные источники: `crates/evohime-core/src/memory_extraction.rs`,
`core_agent_memory.rs`, `core_agent.rs`, `crates/evohime-local-storage/src/
memory_store.rs`, `ambient_store.rs` и существующие migration/IPC diagnostics.

## Этапы

- [Этап 1 — Core contract, schema и storage](./175-1-memory-ingestion-integrity.md)
- [Этап 2 — runtime lifecycle и recovery](./175-2-memory-ingestion-integrity.md)
- [Этап 3 — diagnostics, IPC и projection](./175-3-memory-ingestion-integrity.md)
- [Этап 4 — verification, release evidence и закрытие](./175-4-memory-ingestion-integrity.md)

## Зависимости

### Блокирующие

- Existing memory extraction/governance, ambient retention, memory store,
  Core journal, cancellation/recovery and authenticated IPC primitives.
- Existing durable background execution/restart reconciliation owner (plan
  132) is reused for deferred finalization; this plan does not add a second
  queue, scheduler or general lease/fencing authority.
- Existing Model Gateway and restricted tool/policy contexts; extractor cannot
  acquire generic tools or recursively invoke extraction.

### Опциональные

- Existing RAG/provenance validators; отсутствие optional validator даёт typed
  pending/unknown, а не self-promotion.

## Критерии готовности

- [ ] Origin, root execution, depth and reentrancy lease are Core-owned and
  prevent recursive extraction with observable reason codes.
- [ ] Candidates/finalization are durable, idempotent and publish through
  atomic source-basis/revision CAS; old basis cannot overwrite newer result.
- [ ] Derived/retrieved/model data never self-promotes to user authority.
- [ ] Attempts are throttled before expensive context construction; failures do
  not break the user turn.
- [ ] Restart recovery, cancellation, migration and optional-dependency
  fallback are typed, bounded and inspectable without raw memory payload.
- [ ] Tests, canonical docs and release evidence are updated; plan files are
  removed only after the implementation is actually closed.

## Non-goals

Новый retrieval engine, второй memory store, arbitrary tools for extractor,
полный transcript в memory, silent last-write-wins и обязательная блокировка
user reply до non-critical finalization.

## Источник постановки

- issue #153 Memory Ingestion Integrity (исторический идентификатор постановки)
