# План 171.3 — Language Intelligence Runtime

Статус: этап 3 для [плана 171.0](./171-0-language-intelligence-runtime.md); issue: [#151](https://github.com/rkfsociety/EvoHime/issues/151).

## Зависимости

### Блокирующие

- План 171.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance, cancellation и authenticated IPC boundaries.

### Опциональные

- #102/#104 и diagnostics; при отсутствии их evidence результат остаётся explicit Unknown/degraded, без optimistic pass.

## Реализация

Добавить additive authenticated IPC commands/events после проверки текущего highest tag. Сохранить correlation/idempotency, sequence replay/resync, bounded errors и redaction. Electron показывает только Core-derived metadata, provenance, status и next actions; renderer не получает secrets/raw external data и не исполняет доменную логику.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и Core-validated.
- [ ] Stale/conflict/restart/failure cases имеют безопасный non-success outcome.
- [ ] Нет обхода existing authority, secrets, raw user data или approval.
- [ ] Есть reproducible tests/evidence и rollback/recovery path.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация.

