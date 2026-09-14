# План 170.2 — Multi-Reviewer Ensemble & Adjudication

Статус: этап 2 для [плана 170.0](./170-0-multi-reviewer-ensemble-adjudication.md); issue: [#150](https://github.com/rkfsociety/EvoHime/issues/150).

## Зависимости

### Блокирующие

- План 170.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance, cancellation и authenticated IPC boundaries.

### Опциональные

- #102/#104 и diagnostics; при отсутствии их evidence результат остаётся explicit Unknown/degraded, без optimistic pass.

## Реализация

Подключить Multi-Reviewer Ensemble & Adjudication к Core через typed commands и state machine. Реализовать supervised lifecycle, policy/capability/approval checks, timeout/cancellation, revision fencing, crash recovery и безопасную деградацию. Interrupted/unknown/stale/denied/conflicting operations не объявляются успешными; active runs pin-ят exact revision/identity.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и Core-validated.
- [ ] Stale/conflict/restart/failure cases имеют безопасный non-success outcome.
- [ ] Нет обхода existing authority, secrets, raw user data или approval.
- [ ] Есть reproducible tests/evidence и rollback/recovery path.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация.
