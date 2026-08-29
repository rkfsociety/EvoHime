# План 27.4 — Retained Child Contexts и mailbox: verification, release-evidence и закрытие

Статус: самостоятельный этап 4 для [плана 27.0](./27-0-retained-child-contexts.md); начинается после [плана 27.3](./27-3-retained-child-contexts.md).

## Зависимости

### Блокирующие

- [27.3](./27-3-retained-child-contexts.md) и фактически зелёные
  Core/storage/runtime/IPC/client tests;
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`,
  `docs/decision-register.md`, `docs/release-evidence.md` и действующие release,
  security/eval gates.

### Опциональные

- Goal/Continuation integrations расширяют matrix только после базового MVP;
  отсутствие optional provider остаётся typed `unavailable`.

## Evidence matrix

- Contract: schema/hash/transitions/bounds/error codes и unknown-field rejection.
- Storage: fresh/legacy migration, backup/rollback, parent isolation,
  corruption, atomic sequence и idempotency.
- Runtime: retain, idle follow-up, busy queue, auto/steer policy, cancellation,
  lease/boot recovery, stale/invalidation, expiry/delete и unknown delivery.
- IPC/client: additive tags, Rust/C#/Electron compatibility, replay/resync,
  stale mutation, redaction и old-client behavior.
- Security: sender spoofing, sibling escape, grant escalation, traversal,
  secret/sensitive leakage, imported/untrusted payload и unsafe fallback.

## Обязательная проверка

1. Запустить focused Rust Core/storage/desktop-ipc tests и C# compatibility suite.
2. Запустить `cargo fmt --all -- --check`, relevant `cargo clippy --all-targets
   -- -D warnings`, `cargo check -p evohime-supervisor`,
   `npm run check:protocol`, `npm run typecheck`, relevant `npm test` и
   `git diff --check`; зафиксировать exact command/result/date.
3. Выполнить fault/restart matrix с redacted fixtures; ни один `Unknown` effect
   не объявлять успешным и не повторять blind.

## Release-evidence и закрытие

Evidence допускает только commit, contract/schema versions, test IDs, hashes,
typed outcomes, bounded metrics и recovery state. Credentials, raw provider
output, prompts, transcripts, absolute paths и PII исключены. Зафиксировать
rollback/disable и recovery procedure, включая feature gate и сохранение старых
child rows.

После свежих проверок обновить архитектурный контракт, confirmed state,
development plan, decision register и release evidence. Если направление
полностью подтверждено, перенести контракт и состояние в canonical docs, затем
удалить весь комплект `27-0`…`27-4` по правилу `docs/plans/README.md`. Если хотя
бы один blocking criterion не выполнен, оставить комплект и записать blocked
reason, evidence и следующее действие.

## Definition of Done

- [ ] Все acceptance/evidence matrix подтверждены воспроизводимыми тестами.
- [ ] Нет blocking dependency, implicit downgrade или stale documentation claim.
- [ ] Registry/mailbox recovery и redaction подтверждены на fresh и legacy DB.
- [ ] Compatibility clients не получают authority и не ломаются additive change.
- [ ] Решение `implemented` или `blocked` записано с причиной и следующим шагом.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/8)
