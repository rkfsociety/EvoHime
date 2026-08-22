# 11-4 — Forget, recovery и acceptance

## Цель

Подтвердить полный lifecycle memory/RAG на deterministic fixtures: expiry,
deletion, forget с tombstone, rollback и безопасный degraded behavior.

## Зависимости

### Блокирующие

- 11-1, 11-2 и 11-3 с их Rust fixtures;
- миграция schema v29 → v30 с backup и rollback;
- generated protocol синхронизирован между Rust и Electron.

### Опциональные

- embeddings. При их отсутствии vector-ветки матрицы помечаются skip по
  существующему правилу и не подменяются зелёным результатом FTS5-прогона;
- реальный provider для reflection: в acceptance используется
  `PrecomputedSummaryModel` или deterministic summarizer.

## Deterministic acceptance matrix

### Запись и lifecycle

- observation/tool receipt → retrieval score breakdown → cited summary;
- scope/privacy denial и cross-workspace isolation;
- model assertion без evidence остаётся non-retrievable;
- session note истекает и не становится durable memory;
- supersession chain и transition_state без потери предыдущей ревизии.

### Retrieval

- deterministic tie-break и идентичный replay;
- stale generation и missing citation;
- FTS5-only fallback при отсутствии embeddings;
- фильтрация scope/privacy до ranking.

### Forget и recovery

- expiry, deletion и forget удаляют запись, projection и embeddings;
- после forget нет orphan embeddings и derived summaries, остаётся tombstone;
- stale snapshot, provider failure и cancelled reflection;
- bounded context budget и linkage compaction → source events;
- redaction secrets/PII до SQLite и до UI;
- migration, backup и rollback сохраняют unrelated data.

## Проверки и команды

- Rust: `cargo test --locked -p evohime-core -p evohime-local-storage`
  и targeted memory/RAG/context tests;
- Electron из `desktop/evohime-electron`: `npm run check:protocol`,
  `npm run typecheck`, `npm test`;
- `cargo fmt --check`, `git diff --check`;
- SQLite migration, backup, rollback и retention smoke.

Каждый acceptance test указывает fixture, expected typed code, число
записей/embeddings до и после и итоговую projection. Простого `ok` без
проверки фактического state transition недостаточно.

## Закрытие

После прохождения матрицы контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, затем удаляются все
файлы плана 11. Удаление допустимо только после task-only commit с тестовыми
доказательствами; незавершённый lifecycle остаётся описанным в плане и не
считается закрытым.
