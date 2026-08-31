# План 58.4 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history: verification, release-evidence и закрытие

Статус: этап 4 для [плана 58.0](./58-0-workspace-state-checkpoints.md); после [плана 58.3](./58-3-workspace-state-checkpoints.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 58.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Workspace checkpoint отделён от TaskCheckpoint.
- [ ] Snapshot backend не загрязняет пользовательский Git history.
- [ ] Можно сравнить checkpoint с текущим workspace.
- [ ] Есть независимые RestoreWorkspace/RestoreTask/RestoreBoth flows.
- [ ] Preflight защищает user/external changes от тихого overwrite.
- [ ] Snapshot storage content-addressed/bounded и sensitivity-aware.
- [ ] Restore journaled и recoverable.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Runtime/recovery/fault-injection tests для cancel, stale, denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal, escalation, secret leakage и untrusted input.
6. Real-workspace tests for dirty user edits, `.git` non-mutation, reparse
   escape, bounded ArtifactStore quota and partial restore reconciliation.
7. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `58-0` … `58-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/38)
