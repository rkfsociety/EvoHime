# План 26.4 — Continuation Policy и quality gates: verification, release-evidence и закрытие

Статус: самостоятельный этап 4 для [плана 26.0](./26-0-continuation-policy.md); начинается после [плана 26.3](./26-3-continuation-policy.md).

## Цель

Проверить весь контракт сквозным образом, зафиксировать redacted evidence, rollback/disable procedure и только затем признать направление реализованным или обоснованно заблокированным.

## Зависимости

### Блокирующие

- План 26.3 — завершённые Core, IPC и client projection surfaces.
- Действующие release-evidence, documentation и security/eval gates проекта.

### Опциональные

- Связанные планы из overview расширяют matrix только после прохождения базовых критериев.

## Матрица критериев

- [ ] Continue/Stop выбирает Core по typed evidence, не свободный текст модели.
- [ ] required gates и Goal criteria обязательны для Complete.
- [ ] grants/approvals не расширяются, shell strings не выполняются из policy.
- [ ] fingerprint/no-progress/unknown outcome предотвращают бессмысленные retries.
- [ ] counters, snapshot, stop reason и pending approval переживают restart.
- [ ] user stop немедленно блокирует новые continuations.
- [ ] UI объясняет каждое продолжение и остановку.
- [ ] проходят tests на retryable/non-retryable, budgets, approval, stale event, immutable snapshot и recovery.

## Обязательная проверка

1. Unit/contract tests для schema, canonical hash, state transitions, bounds и error codes.
2. Storage/migration tests с backup, rollback, duplicate/idempotency и corruption cases.
3. Runtime integration/recovery/fault-injection tests для cancellation, stale state, policy denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для authorization, redaction, replay/resync и optimistic conflicts.
5. Security/eval tests на traversal, capability escalation, secret leakage, imported/untrusted content и unsafe fallback; набор выбирается по критериям направления.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy с -D warnings, npm run check:protocol, npm run typecheck, релевантный npm test и git diff --check.

## Evidence matrix

- Contract/storage: `cargo test -p evohime-core -p evohime-local-storage
  -p evohime-desktop-ipc` с именованными fixture IDs и migration evidence.
- Runtime: deterministic continuation/fault fixtures с отдельными outcome
  `Continue`, `Complete`, `PauseForApproval`, `Blocked`, `BudgetLimited`,
  `StopFailed` и `StopUser`; crash/restart evidence содержит только metadata.
- Desktop: `npm run check:protocol`, `npm run typecheck` и релевантные
  `npm test` cases для replay, stale action, redaction и projection.
- Release: evidence фиксирует exact commit, schema/proto versions, policy hash,
  fixture IDs и redaction status; абсолютные пути, secrets, PII и raw provider
  output исключены.

## Release-evidence и закрытие

- Evidence содержит commit, contract/schema versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw provider output, transcripts, absolute paths и PII исключены.
- Зафиксировать rollback/disable и recovery procedure; side effects с unknown outcome не объявлять успешными и не повторять вслепую.
- Обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md только после свежих проверок.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `26-0` … `26-4` согласно docs/plans/README.md; отдельные stage files до этого не удалять. Если критерий не выполнен, оставить направление blocked с причиной и evidence.

## Definition of Done

- [ ] Все критерии выше подтверждены reproducible tests/evidence.
- [ ] Нет незакрытых blocking dependencies или implicit downgrade.
- [ ] Документация и ссылки указывают на фактические версии и пути.
- [ ] В release bundle отсутствуют secrets/PII/raw payload.
- [ ] Решение implemented или blocked записано с причиной и следующим действием.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/6)
