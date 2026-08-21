# План 11 — Typed memory и Core-first RAG

## Цель

Развить существующий Local Agentic RAG/SQLite Core-first слой без создания
второй базы знаний и без автоматического запоминания всего transcript.

## Что уже есть в checkout

- `workspace_rag.rs` с bounded indexing, SQLite/FTS5 generation и citations;
- optional hybrid/vector retrieval с FTS5 fallback;
- context budget/ledger и provenance-aware evidence;
- Core-owned memory storage, consent/policy gates и Electron projection;
- transactional SQLite migration, backup и rollback.

План 11 закрывает единый memory lifecycle и связывает его с execution ledger,
не заменяя существующий RAG новым memory SDK.

## Границы

Входит: typed records, scope/consent/provenance/confidence/TTL, evidence links,
scratch/context/durable separation, deterministic retrieval, embeddings,
compaction, expiry/deletion/forget и plan preview.

Не входит: автоматическое запоминание всего transcript, thought без evidence
как факт, внешняя knowledge base или UI как источник истины.

## Зависимости

### Блокирующие

- планы 08–10 для execution events, policy, scope и authenticated projection;
- текущие `workspace_rag.rs`, memory stores, context ledger и SQLite schema.

### Опциональные

- local embeddings; без них retrieval работает через deterministic FTS5;
- provider reflection; без него compaction завершается deterministic degraded/
  unknown и не подтверждает новые факты.

## Этапы

- [11-1 — typed memory lifecycle](11-1-memory-contract.md)
- [11-2 — evidence и deterministic retrieval](11-2-evidence-retrieval.md)
- [11-3 — context budget, compaction и projections](11-3-context-compaction.md)
- [11-4 — forget, recovery и acceptance](11-4-memory-acceptance.md)

Порядок: 11-1 → 11-2 → 11-3 → 11-4.

## Готово, когда

Каждая memory record имеет provenance и lifecycle, retrieval объясним и
воспроизводим, forget удаляет все derived data, compaction сохраняет ссылки на
исходные events, а UI меняет только projection.
