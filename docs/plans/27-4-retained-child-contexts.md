# План 27.4 — Retained Child Contexts и mailbox: verification, release-evidence и закрытие

Статус: самостоятельный этап 4 для [плана 27.0](./27-0-retained-child-contexts.md); начинается после [плана 27.3](./27-3-retained-child-contexts.md).

## Цель

Проверить весь контракт сквозным образом, зафиксировать redacted evidence, rollback/disable procedure и только затем признать направление реализованным или обоснованно заблокированным.

## Зависимости

### Блокирующие

- План 27.3 — завершённые Core, IPC и client projection surfaces.
- Действующие release-evidence, documentation и security/eval gates проекта.

### Опциональные

- Связанные планы из overview расширяют matrix только после прохождения базовых критериев.

## Матрица критериев

- [ ] parent-scoped registry и typed follow-up contract durable.
- [ ] mailbox доставляет/очередит bounded messages с duplicate protection.
- [ ] grants, context, provenance, revision и freshness проверяются на каждом run.
- [ ] formal child report/fan-in остаётся отдельным контрактом.
- [ ] restart, pending delivery, expiry, deletion и uncertain outcome recoverable.
- [ ] sibling escape, secret payload и raw transcript не попадают в UI.
- [ ] limits/overflow дают явный typed outcome.
- [ ] проходят child, storage, recovery, IPC и security tests.

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

- [issue](https://github.com/rkfsociety/EvoHime/issues/8)
