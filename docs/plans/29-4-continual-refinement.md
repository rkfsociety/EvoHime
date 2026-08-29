# План 29.4 — Continual Refinement с evidence и approval: verification, release-evidence и закрытие

Статус: этап 4 для [плана 29.0](./29-0-continual-refinement.md); после [плана 29.3](./29-3-continual-refinement.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 29.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] repeated evidence создаёт candidate, единичная ошибка не создаёт global rule.
- [ ] candidate имеет scope, provenance, content hash и durable lifecycle.
- [ ] duplicate/conflict/security/eval failure блокируют unsafe activation.
- [ ] global activation требует explicit approval.
- [ ] activated versions имеют rollback и before/after history.
- [ ] refinement не расширяет capabilities и не меняет security policy.
- [ ] UI показывает bounded queue/history/diff без sensitive raw content.
- [ ] provenance переживает restart, а tests покрывают memory/skill/prompt paths.

### Evidence matrix

- `R29-C01`: fixtures с одной ошибкой, повтором в одной задаче, независимыми
  task ids и threshold/retention policy.
- `R29-C02`: contract/hash/schema/storage tests с immutable revision и bounds.
- `R29-C03`: duplicate, conflict, revoked/deleted source, insufficient
  evidence, security rejection и evaluation failure.
- `R29-C04`/`R29-C05`: memory API, SkillRegistry и PromptRule adapter; explicit
  approval для global/high-risk; typed `unavailable` для отсутствующего target.
- `R29-C06`: crash до/после dispatch, stale revision, idempotent replay,
  before/after history и rollback без blind retry.
- `R29-C07`: authenticated IPC, generated protocol, reconnect/replay gap,
  redacted renderer projection и stale action.
- `R29-C08`: secret/sensitive leakage, self-escalation, raw transcript,
  retention/forget, restart provenance и package/release evidence.

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
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `29-0` … `29-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.
- [ ] Evidence содержит команды, commit, schema/protocol versions, test IDs,
  hashes и typed outcomes; отсутствуют credentials, raw transcript, sensitive
  candidate body и абсолютные пути.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/4)
