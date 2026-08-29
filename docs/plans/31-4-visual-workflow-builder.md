# План 31.4 — Visual Workflow Builder: typed canvas, validation и live runtime inspection: verification, release-evidence и закрытие

Статус: этап 4 для [плана 31.0](./31-0-visual-workflow-builder.md); после [плана 31.3](./31-3-visual-workflow-builder.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 31.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Есть canvas над существующим workflow contract.
- [ ] Pins и block metadata приходят из Core registry.
- [ ] Core выполняет authoritative validation.
- [ ] Сохранение создаёт immutable новую version.
- [ ] Layout metadata отделена от execution hash.
- [ ] Есть recovery draft.
- [ ] Есть read-only live runtime inspection.
- [ ] Sensitive payload не утекает в renderer.
- [ ] Есть bounded versioned handoff для Composer без второго save/run authority.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Draft/recovery/fault-injection tests для stale, denial, duplicate, restart,
   corruption, invalidated Composer handoff и immutability published/running graph.
4. IPC/adapter/renderer tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal, escalation, secret leakage и untrusted input.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и draft recovery procedure записаны; Builder не dispatch-ит side effect, а live inspection не меняет run и не повторяет dispatch.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `31-0` … `31-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/11)
