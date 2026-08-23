# 11-4 — Forget, recovery и acceptance

## Цель

Подтвердить полный lifecycle memory/RAG на deterministic fixtures: expiry,
deletion, forget с tombstone, rollback и безопасный degraded behavior.

## Зависимости

### Блокирующие

- 11-1, 11-2 и 11-3 с их Rust fixtures;
- миграция schema v30 → v31, применённая к базам v26, v28 и v30, с
  pre-migration backup;
- generated protocol синхронизирован между Rust и Electron.

### Опциональные

- hybrid-ветка retrieval. Эмбеддер `embed_local` детерминирован и доступен
  всегда, поэтому vector-ветки матрицы не пропускаются по «отсутствию
  embeddings»; пропуск допустим только при явно отключённом
  `HybridConfig.enabled` и помечается skip по существующему правилу, а не
  подменяется зелёным результатом FTS5-прогона;
- реальный provider для reflection: в acceptance используется
  `PrecomputedSummaryModel` или deterministic summarizer.

## Deterministic acceptance matrix

### Запись и lifecycle

- observation/tool receipt → retrieval score breakdown → cited summary;
- scope/privacy denial и cross-workspace isolation;
- `privacy_class == "secret"` отвергается с `SecretNotStorable` до записи;
- model assertion без evidence остаётся non-retrievable;
- session note истекает через `purge_expired_session_notes`, а запись со
  `MemoryScope::Session` не попадает в long-term retrieval; ни то, ни другое
  не становится durable memory без явной команды с evidence;
- scratchpad `confirm` не создаёт memory record сам по себе;
- supersession chain и `transition_state` без потери предыдущей ревизии.

### Retrieval

- deterministic tie-break и идентичный replay;
- stale/updated citation и generation mismatch;
- `fallback_fts5` с причинами `vector_index_unavailable` и
  `vector_index_incompatible`;
- фильтрация scope/privacy до ranking: добавление записи вне scope не меняет
  порядок остальных.

### Forget и recovery

- expiry, deletion и forget делают memory record логически forgotten (текущий
  store сохраняет обезличенную metadata/state-строку для tombstone-а), удаляют
  все derived projection и связанные вектора;
- после forget не остаётся derived summaries, aliases, RAG/context ledger
  projections и строк `workspace_chunk_vectors`, ссылающихся на удалённый
  chunk или index
  (каскад `ON DELETE CASCADE` не обойдён), остаётся tombstone;
- stale snapshot, provider fallback (`CompressionRecord.fallback = true`) и
  cancelled reflection;
- bounded context budget и linkage compaction → исходные `sequence_id`;
  `prune` не удаляет события, на которые ссылается живая summary;
- redaction secrets/PII до SQLite и до UI;
- миграция и восстановление из pre-migration копии (`.db.bak` /
  `rollback_from_safety`) сохраняют unrelated data; downgrade
  `user_version` не поддерживается и даёт `UnsupportedSchema`.

## Проверки и команды

- Rust: `cargo test --locked -p evohime-core -p evohime-context-budget
  -p evohime-local-storage` и targeted memory/RAG/context tests;
- Electron из `desktop/evohime-electron`: `npm run check:protocol`,
  `npm run typecheck`, `npm test`;
- `cargo fmt --check`, `git diff --check`;
- SQLite migration, backup, восстановление и retention smoke.

Каждый acceptance test указывает fixture, expected typed code, число
записей/векторов до и после и итоговую projection. Простого `ok` без
проверки фактического state transition недостаточно.

## Закрытие

После прохождения матрицы контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, затем удаляются все
файлы плана 11. Удаление допустимо только после task-only commit с тестовыми
доказательствами; незавершённый lifecycle остаётся описанным в плане и не
считается закрытым.
