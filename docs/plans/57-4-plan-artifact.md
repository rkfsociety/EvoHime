# План 57.4 — Plan Artifact: versioned planning contract и явный переход Plan → Execute: verification, release-evidence и закрытие

Статус: этап 4 для [плана 57.0](./57-0-plan-artifact.md); после [плана 57.3](./57-3-plan-artifact.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 57.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Есть versioned `PlanArtifact`/`PlanStep` contract с identity, revision, hash и provenance.
- [ ] Acceptance criteria типизированы, имеют `evidence_kind`, а их status выводится из Core-owned evidence, не из `done=true` модели.
- [ ] Переход `Plan -> Execute` выполняется только Core-командой до первого side effect.
- [ ] Plan steps разрешают capabilities через Core registry и не несут raw executable identity как authority.
- [ ] Accepted plan revision/hash immutable; material deviation требует revalidation или replan.
- [ ] Execution/recovery сохраняют unknown outcome и не повторяют внешний effect вслепую.
- [ ] Plan, criteria и evidence переживают restart в bounded durable state.
- [ ] IPC/UI показывают только bounded redacted projection и явные actions.

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
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `57-0` … `57-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/37)
