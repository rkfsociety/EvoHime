# План 119.4 — Execution Environment Profiles: verification, release-evidence и закрытие

Статус: этап 4 для [плана 119.0](./119-0-execution-environment-profiles.md); после [плана 119.3](./119-3-execution-environment-profiles.md).

## Цель

Подтвердить требования issue #99 свежими воспроизводимыми тестами, оформить redacted release evidence и перенести фактически реализованный contract в canonical documentation.

## Зависимости

### Блокирующие

- План 119.3 — полный Core/resolver/activation/IPC/UI vertical slice.
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`, release-evidence и security/eval gates.

### Опциональные

- Local Model Runtime, Context refs, CLI и Diagnostics Bundle integrations.

## Матрица проверки

- profile revision/hash, typed refs, scope/layer precedence, duplicate/conflict/unknown binding, required/optional state and no duplicated owner state;
- valid all-required activation, missing required atomic block, missing optional Degraded, capability/model mismatch, provider/external/MCP/workbench/skill/instruction/policy/credential diagnostics;
- pinned revision stability, FollowCompatible recorded resolution, drift NeedsReview/Broken, LocalOnly/cloud rejection, hard-ceiling preservation and no secret material;
- NewRunOnly/NextTurn/NewConversationOnly boundary selection, active-run immutability, effective snapshot/provenance and no partial switch;
- rollback as new activation event, stale precondition, concurrent activation, unknown outcome reconciliation, restart and last-valid recovery;
- IPC authentication, bounds, replay/resync, duplicate/idempotency, optimistic conflict, renderer-forged-state rejection and metadata-only diff/UI;
- external agent known-adapter boundary, rejection of arbitrary config paths/scripts/executables and imported profile validation/rebinding;
- no credentials, prompts, outputs, raw source, PII, secret literals or hidden reasoning in storage, logs, IPC or release bundle.

## Обязательные gates

1. Focused Core/storage/resolution/activation/recovery/security tests с migration backup, rollback и fault injection.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused environment tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging.
4. `git diff --check`, redaction scan, dependency/provenance review и documented disable/rollback procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol versions, test IDs, hashes, typed outcomes, binding/diagnostic summary and redaction status. Не включать credentials, raw prompts/outputs, secret slots beyond safe ids, absolute paths, PII or provider payloads.

После подтверждения criteria перенести фактический contract в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать rollback: отключение Environment Profiles не ломает existing model/MCP/skill/policy/agent paths; unknown activation не объявляется success и не повторяется вслепую.

После полного закрытия направления удалить комплект `119-0`…`119-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, owner revisions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, rollback/recovery procedure записаны.

## Связанный issue

- [issue #99](https://github.com/rkfsociety/EvoHime/issues/99)
