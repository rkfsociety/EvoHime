# План 118.4 — Persistent Agent Organization Registry: verification, release-evidence и закрытие

Статус: этап 4 для [плана 118.0](./118-0-persistent-agent-organization-registry.md); после [плана 118.3](./118-3-persistent-agent-organization-registry.md).

## Цель

Подтвердить требования issue #98 свежими воспроизводимыми тестами, оформить redacted release evidence и перенести фактически реализованный контракт в канонические документы.

## Зависимости

### Блокирующие

- План 118.3 — полный Core/runtime/IPC/UI vertical slice.
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`, release-evidence и security/eval gates.

### Опциональные

- Model routing, scheduler и diagnostics integrations расширяют evidence, но не заменяют базовую registry matrix.

## Матрица проверки

- create/revise/activate/pause/suspend/retire, stable identity across runs, one profile across agents, exact revision conflicts и no destructive history loss;
- self/cyclic/cross-scope reporting rejection, revisioned history, retired target rejection и proof that hierarchy never widens grants;
- Owner/Contributor/Reviewer exact Goal revision bindings, no copied Goal state and no unauthorized Goal mutation;
- assignment to ordinary child/run/TeamSession, multiple participation, immutable execution agent/profile/goal/accountability snapshot and future-only effect of registry changes;
- derived Ready/Busy/Waiting/Blocked/RuntimeUnavailable from actual state, no stale Busy after restart, broken profile binding and source-linked bounded activity/cost/artifact projection;
- duplicate/unknown/stale assignment outcomes, crash/restart reconciliation, no duplicate run and no false success;
- IPC auth, bounds, replay/resync, idempotency, optimistic conflict, redaction and renderer forgery rejection;
- no credentials, prompts, outputs, transcripts, PII or capability grants in durable records, logs, IPC or release bundle.

## Обязательные gates

1. Focused Core/storage/runtime/recovery/security tests с migration backup, rollback и fault injection.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused registry tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging.
4. `git diff --check`, redaction scan, dependency/provenance review и documented disable/recovery procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol versions, test IDs, hashes, typed outcomes, revision/source summary и redaction status. Не включать credentials, raw conversation, absolute paths, PII, hidden reasoning или duplicate ledger dumps.

После подтверждения criteria перенести фактический contract в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать rollback: отключение Registry не ломает existing Goals/Roles/Teams/runs; unknown assignment outcome не повторяется вслепую.

После полного закрытия направления удалить комплект `118-0`…`118-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, revision versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, rollback/recovery procedure записаны.

## Связанный issue

- [issue #98](https://github.com/rkfsociety/EvoHime/issues/98)
