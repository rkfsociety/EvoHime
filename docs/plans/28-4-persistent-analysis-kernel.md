# План 28.4 — Persistent Analysis Kernel: verification, release-evidence и закрытие

Статус: этап 4 для [плана 28.0](./28-0-persistent-analysis-kernel.md); после [плана 28.3](./28-3-persistent-analysis-kernel.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 28.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] controlled worker/runtime и persistent in-session state существуют.
- [ ] typed Core host bridge — единственный side-effect path.
- [ ] нет unrestricted shell/network/credentials/file access.
- [ ] limits, crash isolation, reset и recovery проверены.
- [ ] object registry, sensitivity/provenance и TaskCheckpoint integration работают.
- [ ] child получает только selected immutable refs.
- [ ] UI/diagnostics bounded и не раскрывает raw values.
- [ ] проходят resource, security, restart, compaction, approval и package manifest tests.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Runtime/recovery/fault-injection tests для cancel, stale, denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal, escalation, secret leakage и untrusted input.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- Завершённый stage удалить после переноса подтверждённого контракта; незавершённый оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/9)
