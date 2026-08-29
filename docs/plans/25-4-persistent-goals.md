# План 25.4 — Persistent Goals для длительных задач: verification, release-evidence и закрытие

Статус: самостоятельный этап 4 для [плана 25.0](./25-0-persistent-goals.md); начинается после [плана 25.3](./25-3-persistent-goals.md).

## Цель

Проверить весь контракт сквозным образом, зафиксировать redacted evidence, rollback/disable procedure и только затем признать направление реализованным или обоснованно заблокированным.

## Зависимости

### Блокирующие

- План 25.3 — завершённые Core, IPC и client projection surfaces.
- Действующие release-evidence, documentation и security/eval gates проекта.

### Опциональные

- Связанные планы из overview расширяют matrix только после прохождения базовых критериев.

## Матрица критериев

- [ ] Goal создаётся явно, durable и имеет versioned status machine.
- [ ] success criteria/evidence отличают подтверждение от текста модели.
- [ ] Goal связывает несколько workflow/child runs и последний checkpoint.
- [ ] stale event не ломает новую projection, recovery не повторяет uncertain effect.
- [ ] budget exhaustion виден как `BudgetLimited`.
- [ ] objective/criteria versioning сохраняет историю.
- [ ] UI позволяет pause/resume/cancel и показывает blockers/next action.
- [ ] Goal не расширяет capabilities и не хранит credentials.
- [ ] проходят storage, recovery, IPC, UI и deterministic transition tests.

## Обязательная проверка

1. Unit/contract tests для schema, canonical hash, state transitions, bounds и error codes.
2. Storage/migration tests с backup, rollback, duplicate/idempotency и corruption cases.
3. Runtime integration/recovery/fault-injection tests для cancellation, stale state, policy denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для authorization, redaction, replay/resync и optimistic conflicts.
5. Security/eval tests на traversal, capability escalation, secret leakage, imported/untrusted content и unsafe fallback; набор выбирается по критериям направления.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy с -D warnings, npm run check:protocol, npm run typecheck, релевантный npm test и git diff --check.

## Release-evidence и закрытие

- Evidence содержит commit, contract/schema versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw provider output, transcripts, absolute paths и PII исключены.
- Зафиксировать rollback/disable и recovery procedure; side effects с unknown outcome не объявлять успешными и не повторять вслепую.
- Обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md только после свежих проверок.
- После переноса подтверждённого контракта удалить завершённые stage files согласно docs/plans/README.md; если критерий не выполнен, оставить blocked с причиной и evidence.

## Definition of Done

- [ ] Все критерии выше подтверждены reproducible tests/evidence.
- [ ] Нет незакрытых blocking dependencies или implicit downgrade.
- [ ] Документация и ссылки указывают на фактические версии и пути.
- [ ] В release bundle отсутствуют secrets/PII/raw payload.
- [ ] Решение implemented или blocked записано с причиной и следующим действием.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/5)
