# 04. Память и RAG

## Цель

Развить существующий Local Agentic RAG/SQLite Core-first слой без создания
второй базы знаний и без автоматического запоминания всего transcript.

## Scope

- typed memory records с type, scope, consent, provenance, confidence и TTL;
- source/evidence links и связь с execution events;
- разделение scratch state, текущего context и durable memory;
- multi-factor retrieval с детерминированным tie-break;
- hybrid retrieval, local embedding cache и bounded context budget;
- expiry, deletion и forget для записей, projections и embeddings;
- cancellable reflection/compaction с budget, snapshot revision и idempotency;
- plan preview до side effect.

## Инварианты

- Источник истины — Core/SQLite и текущая RAG-схема, а не внешний memory SDK.
- Thought без evidence не становится фактом или trusted memory.
- Retrieval обязан возвращать provenance и объяснение выбора.
- Scope и consent проверяются до записи и до выдачи в context.
- Forget удаляет все связанные projections, embeddings и derived summaries.
- Compaction — versioned projection со ссылками на исходные события, а не
  незаметное удаление истории.

## Первый offline fixture

```text
запрос → observation/tool receipt → retrieval с score breakdown
       → draft плана → approval-gated action → summary с citations
```

Проверить также ошибку provider, stale snapshot, отмену reflection и то, что
модельный output с неизвестным фактом остаётся typed unknown.

## Тестовый контур

- scope/consent и cross-workspace isolation;
- deterministic retrieval tie-break;
- expiry, deletion, forget и отсутствие orphan embeddings;
- replay из записанных входов;
- bounded context и memory budget;
- compaction с перечнем исходных event IDs;
- redaction secrets/PII до memory write.

## Критерии готовности

- каждая запись имеет provenance и lifecycle;
- retrieval объясним и воспроизводим;
- forget проверен на всех derived data;
- UI показывает projection и не меняет memory напрямую;
- migration/backup/rollback SQLite подтверждены.

## Зависимости

Требует 01–03. Текущий `workspace_rag.rs` и его schema/tests нужно сначала
сопоставить с этим контрактом, а не переписывать вслепую.
