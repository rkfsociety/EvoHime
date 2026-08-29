# План 23.4 — TaskCheckpoint для compaction и recovery: verification, release-evidence и закрытие

Статус: самостоятельный этап 4 для [плана 23.0](./23-0-task-checkpoint.md); начинается после [плана 23.3](./23-3-task-checkpoint.md).

## Цель

Проверить весь контракт сквозным образом, зафиксировать redacted evidence, rollback/disable procedure и только затем признать направление реализованным или обоснованно заблокированным.

## Зависимости

### Блокирующие

- План 23.3 — завершённые Core, IPC и client projection surfaces.
- Действующие release-evidence, documentation и security/eval gates проекта.

### Опциональные

- Связанные планы из overview могут расширять matrix только после прохождения базовых критериев.

## Матрица критериев

- [ ] versioned `TaskCheckpoint` durable contract и immutable parent chain.
- [ ] Core-derived evidence отделён от model-proposed summary.
- [ ] checkpoint создаётся до compaction и используется после replay recovery.
- [ ] stale workspace, unknown outcome, pending approval и failed gate не теряются.
- [ ] большие и sensitive данные не копируются в checkpoint/renderer.
- [ ] corrupted latest snapshot безопасно заменяется предыдущим + replay.
- [ ] IPC/UI показывают bounded typed projection.
- [ ] проходят storage, compaction, restart, deterministic hash, redaction и `git diff --check`/проектные документационные gates.

## Обязательная проверка

1. Unit/contract tests для schema, canonical hash, state transitions, bounds and error codes.
2. Storage/migration tests с backup, rollback, duplicate/idempotency и corruption cases.
3. Runtime integration/recovery/fault-injection tests для cancellation, stale state, policy denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для authorization, redaction, replay/resync и optimistic conflicts.
5. Security/eval tests на traversal, capability escalation, secret leakage, imported/untrusted content и unsafe fallback; набор выбирается по критериям этого направления.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy с -D warnings, npm run check:protocol, npm run typecheck, релевантный npm test и git diff --check.

## Release-evidence и закрытие

- Evidence содержит commit, contract/schema versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw provider output, transcripts, absolute paths и PII исключены.
- Зафиксировать rollback/disable и recovery procedure; side effects с unknown outcome не объявлять успешными и не повторять вслепую.
- Обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md только после свежих проверок.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `23-0` … `23-4` согласно docs/plans/README.md; отдельные stage files до этого не удалять. Если критерий не выполнен, оставить направление blocked с причиной и evidence.

## Definition of Done

- [ ] Все критерии выше подтверждены reproducible tests/evidence.
- [ ] Нет незакрытых blocking dependencies или implicit downgrade.
- [ ] Документация и ссылки указывают на фактические версии и пути.
- [ ] В release bundle отсутствуют secrets/PII/raw payload.
- [ ] Решение implemented или blocked записано с причиной и следующим действием.

## Связанный issue

- [https://github.com/rkfsociety/EvoHime/issues/7](https://github.com/rkfsociety/EvoHime/issues/7)
