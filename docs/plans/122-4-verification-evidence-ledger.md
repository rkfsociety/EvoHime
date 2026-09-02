# План 122.4 — Verification Evidence Ledger: verification, release-evidence и закрытие

Статус: этап 4 для [плана 122.0](./122-0-verification-evidence-ledger.md); после [плана 122.3](./122-3-verification-evidence-ledger.md).

## Цель

Подтвердить требования issue #102 свежими воспроизводимыми тестами, оформить redacted release evidence и перенести фактически реализованный contract в canonical documentation.

## Зависимости

### Блокирующие

- План 122.3 — полный Core/fingerprint/runner/freshness/readiness/IPC/UI vertical slice.
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`, release-evidence и security/eval gates.

### Опциональные

- Architecture Snapshot, Code Diagnostics, Change Sets, Environment Profiles and Diagnostics Bundle integrations.

## Матрица проверки

- clean/dirty/staged/unstaged/untracked/ignored workspace identity, same-content commit/rebase/amend, distinct worktree/root, scoped selective reuse and conservative unknown dependency;
- lane/command/provider revisions, exact argv/cwd/env/capability/timeout/result contract, trust change review and no arbitrary model shell;
- Passed/Failed/Unknown/Unavailable/Cancelled/TimedOut/ProtocolError/Invalidated outcomes for exit/provider/fingerprint/artifact conditions;
- workspace mutation before/after, missing executable, non-zero, timeout, cancellation, malformed/empty reviewer result, unknown transport, missing artifact ref and redaction;
- exact freshness, stale reasons, age/policy/command/environment/artifact invalidation, required/optional/conditional lanes and no model-selected mandatory-lane removal;
- reviewer independence classes: human/deterministic/provider/distinct family/same family/self review and policy enforcement;
- readiness Ready/Blocked/NeedsVerification/NeedsHumanReview/Incomplete/Unknown, skipped/unavailable required blocking, explicit override distinct from PASS;
- Continuation/Termination/Goal/Task/Plan/Change Set consumers, bounded correction loop and no duplicate readiness implementation;
- restart/reconcile Running/Unknown, last-good state, cancellation, retry guards and ArtifactStore/Handoff metadata-only boundary;
- IPC auth/bounds/replay/resync/idempotency, renderer forged Pass/Fresh/Ready rejection and no sensitive projection.

## Обязательные gates

1. Focused Core/storage/fingerprint/runner/freshness/readiness/recovery/security tests with migration backup, rollback and fault injection.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused ledger tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging.
4. `git diff --check`, redaction scan, dependency/provenance review и documented retention/disable/recovery procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol/lane/executor versions, test IDs, hashes, typed outcomes, freshness/readiness transitions and redaction status. Не включать credentials, secrets/env values, private source/output, raw reviewer prompts, PII, absolute paths or unrestricted logs.

После подтверждения criteria перенести фактический contract в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать rollback: отключение Ledger оставляет existing verifier/artifact/continuation paths работоспособными; unavailable/unknown evidence не объявляется success и не повторяется вслепую.

После полного закрытия направления удалить комплект `122-0`…`122-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, executor/lane versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, retention/rollback/recovery procedure записаны.

## Связанный issue

- [issue #102](https://github.com/rkfsociety/EvoHime/issues/102)
