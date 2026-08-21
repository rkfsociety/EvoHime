# 11-3 — Context budget, compaction и projections

## Цель

Управлять context budget и reflection/compaction как cancellable versioned
projection без незаметного удаления истории.

## Изменения

1. Связать retrieval output с существующим context ledger и bounded context
   budget; обязательные элементы и причины pruning должны быть видимы.
2. Реализовать cancellable reflection/compaction с budget, snapshot revision,
   idempotency и deterministic fallback при provider failure.
3. Сохранять derived summary/embedding как versioned projection со ссылками на
   исходные event IDs, а не заменять ими историю.
4. Применять redaction до memory write и до context projection; raw prompt и
   full tool output не становятся durable memory автоматически.
5. Отдавать UI только metadata, score breakdown, citations и bounded preview;
   mutation выполняется через Core command path.

## Проверки

- context budget overflow и deterministic pruning;
- compaction cancel, retry, stale snapshot и idempotent повтор;
- provider failure с degraded/unknown результатом;
- source event linkage для каждого summary;
- reconnect/replay projection после Core restart.

## Готово, когда

Compaction не теряет происхождение данных, соблюдает budget/cancellation и не
может скрыто удалить исходные execution или evidence events.
