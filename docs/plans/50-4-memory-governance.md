# План 50.4 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy: verification, release-evidence и закрытие

Статус: этап 4 для [плана 50.0](./50-0-memory-governance.md); после [плана 50.3](./50-3-memory-governance.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 50.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] MemoryRecord имеет typed kind/scope/durability/authority/confidence.
- [ ] Долговременные записи проходят MemoryWriteGate.
- [ ] Есть dedup/merge и explicit contradiction semantics.
- [ ] Reinforcement требует независимого evidence.
- [ ] Есть freshness и versioned retention policy.
- [ ] Retrieval сохраняет provenance/authority metadata.
- [ ] Пользователь может инспектировать и исправлять память.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Runtime/recovery/fault-injection tests для cancel, stale, denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal, escalation, secret leakage и untrusted input.
6. Migration/coexistence tests: ни один legacy create/update/extraction path не
   обходит gate; shadow и enforcement дают ожидаемые deterministic decisions.
7. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `50-0` … `50-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/30)
