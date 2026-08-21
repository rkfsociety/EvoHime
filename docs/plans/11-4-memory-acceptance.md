# 11-4 — Forget, recovery и acceptance

## Цель

Подтвердить полный lifecycle memory/RAG, включая expiry, deletion, forget,
rollback и безопасный degraded behavior.

## Deterministic acceptance fixtures

- observation/tool receipt → retrieval score breakdown → cited summary;
- scope/consent denial и cross-workspace isolation;
- unknown model fact без evidence;
- expiry, deletion и forget записи, projection и embeddings;
- отсутствие orphan embeddings и derived summaries после forget;
- stale snapshot, provider failure и cancelled reflection;
- bounded context budget и compaction event linkage;
- redaction secrets/PII до SQLite и UI;
- migration/backup/rollback с сохранением unrelated data.

## Release checks

- targeted Rust Core, local-storage, workspace-RAG и memory tests;
- deterministic fixtures/replay и security policy tests;
- Electron projection/IPC tests;
- `cargo fmt --check`, `git diff --check`, `npm run check:protocol`,
  `npm run typecheck`, `npm test`;
- SQLite migration, backup, rollback и retention smoke.

## Закрытие

После прохождения критериев контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, а план 11 удаляется
только после полной проверки. Незавершённый lifecycle остаётся описанным в
плане и не считается закрытым.
