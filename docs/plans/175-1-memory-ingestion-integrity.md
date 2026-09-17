# План 175.1 — Memory Ingestion Integrity: Core, schema и storage

## Зависимости

### Блокирующие

- [Обзор плана 175](./175-0-memory-ingestion-integrity.md).
- `MemoryExtractionFields`, `memory_entries`, ambient store, provenance
  validators и существующая migration/backup infrastructure.

### Опциональные

- Existing artifact/event stores for bounded diagnostic references.

## Реализация

- Ввести typed `MemoryExtractionOrigin`, eligibility, root/depth metadata,
  ingestion-scoped extraction lease, candidate journal и finalization state;
  origin не выводить из prompt/model metadata.
- Candidate/finalization rows остаются у существующего memory storage owner;
  Core journal получает только metadata-only lifecycle events и не становится
  второй memory store.
- Добавить `MemorySourceBasis` и `MemoryPublishPrecondition` с source revision,
  event/turn/hash basis, expected record/head revision и idempotency key.
- Расширить storage transaction primitives: candidate capture, lease acquire/
  expiry, CAS publish, supersession/conflict outcomes и durable deferred/
  failed terminal state. Idempotency проверять в той же transaction.
- Выполнить additive schema migration через существующий storage owner,
  сохранив старые memory rows, ambient deletion/tombstone semantics и быстрый
  startup migration без inline heavy index rebuild.

## Критерии

- Один source basis не может быть опубликован дважды semantic-copy или поверх
  более свежего active head.
- `Committed`, `AlreadyCommitted`, `StaleBasis`, `RevisionConflict`,
  `SupersededByNewer`, `RejectedByPolicy` сериализуются bounded и устойчивы к
  повторному запуску.
- Migration upgrade/rollback/fresh install и cleanup expired leases покрыты
  storage tests; raw statement/transcript не нужен для lifecycle diagnosis.
- Lease expiry and idempotency use the existing generation/fencing semantics;
  an extraction lease must not become a second global lease manager.

## Non-goals

Изменение существующих candidate policy thresholds, новый индекс поиска и
сетевой extractor execution.
