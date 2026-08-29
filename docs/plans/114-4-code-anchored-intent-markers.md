# План 114.4 — Code-Anchored Intent Markers: задачи и вопросы Еве прямо из комментариев в исходниках: verification, release-evidence и закрытие

Статус: этап 4 для [плана 114.0](./114-0-code-anchored-intent-markers.md); после [плана 114.3](./114-3-code-anchored-intent-markers.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 114.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Есть typed CodeIntentMarker contract/lifecycle.
- [ ] Поддержаны Question и EditRequest markers.
- [ ] Parsing выполняется по comment ranges, а не raw whole-file regex в auto mode.
- [ ] Marker привязан к exact file revision/range и optional semantic symbol.
- [ ] Есть trusted authorship/provenance classification.
- [ ] Existing/agent-generated content не auto-trigger-ится.
- [ ] Есть debounce/dedup/stale/loop protection.
- [ ] Marker запускает обычный EvoHime task, не отдельный небезопасный runtime.
- [ ] Workbench/conversation умеет перейти marker -> run -> diff/result.

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

- [issue](https://github.com/rkfsociety/EvoHime/issues/94)
