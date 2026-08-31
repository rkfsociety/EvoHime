# План 60.4 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection: verification, release-evidence и закрытие

Статус: этап 4 для [плана 60.0](./60-0-revision-safe-workspace-files.md); после [плана 60.3](./60-3-revision-safe-workspace-files.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 60.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Есть typed namespaces uploads/workspace/outputs/scratch.
- [ ] File refs несут content hash/revision.
- [ ] Mutations поддерживают expected hash/revision preconditions.
- [ ] Stale write никогда не применяется молча.
- [ ] Uploads immutable по умолчанию, scratch run-scoped.
- [ ] После изменений создаётся observed WorkspaceChangeSet.
- [ ] External edits инвалидируют stale refs.
- [ ] Path traversal/symlink/reparse escape закрыты.
- [ ] Workbench/Artifact layers используют refs, а не обходят Core file semantics.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Runtime/recovery/fault-injection tests для cancel, stale, denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal, escalation, secret leakage и untrusted input.
6. Real legacy-tool migration tests for read/write/patch/move/delete, fuzzy
   patch rejection, host-path redaction, two-writer race, external drift,
   reparse swap and partial batch recovery.
7. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `60-0` … `60-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/40)
