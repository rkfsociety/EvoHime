# План 123.4 — Content-Aware Context Compression: verification, benchmark, release-evidence и закрытие

Статус: этап 4 для [плана 123.0](./123-0-content-aware-context-compression.md); после [плана 123.3](./123-3-content-aware-context-compression.md).

## Цель

Подтвердить требования issue #103 свежими воспроизводимыми contract/runtime/quality/security тестами, оформить redacted evidence и перенести фактически реализованный contract в canonical documentation.

## Зависимости

### Блокирующие

- План 123.3 — полный Core/compactor/recovery/Context/IPC/UI vertical slice.
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`, release-evidence и security/eval gates.

### Опциональные

- Agent Benchmark Matrix, Prompt Cache, RAG/Map, Diagnostics Bundle and ContextRefs.

## Матрица проверки

- classification for JSON/JSONL/log/test/diff/search/HTML/accessibility/source, malformed/low-confidence conservative behavior and registered hint security;
- deterministic log/build/test compaction preserving errors/failures/stacks/counters, JSON schema/errors/distinct records, every diff changed line/hunk and security-priority search results;
- exact source revision/hash/omitted locators/visible markers, source ownership, retention deletion and RecoveryUnavailable;
- typed exact/around/path/line/page/group recovery, stale/wrong hash, sensitivity/provider/authority recheck, bounded repeated/cascading recovery and no arbitrary file access;
- positive/negative/no-benefit token gate including compact/instruction/recovery overhead, estimate/provider/counterfactual metrics and negative cases;
- normal-vs-compressed paired benchmark on noisy logs, critical error, repetitive JSON, CSV/TSV, diff, search and unsupported/no-benefit payloads; quality/evaluator result, input/output/recovery tokens, latency and compaction cost;
- protected system/safety/approval/user-constraint/verifier classes remain lossless; prompt injection/untrusted HTML/log/JSON stays data; security/redaction failure has no raw fallback;
- Context Budget/Ledger/ContextRefs/Prompt Cache lineage, source→compact→recovered→summary and failure fallback remain consistent;
- IPC auth/bounds/replay/resync/idempotency, renderer forged hash/savings/recovery rejection, metadata-only diagnostics and accessibility states;
- no secrets, raw source/prompt/output, arbitrary handles, model credentials or unbounded corpus in storage/logs/IPC/release bundle.

## Обязательные gates

1. Focused Core/storage/classifier/compactor/recovery/context integration/quality/security tests with migration backup, rollback and fault injection.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused compression tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging.
4. `git diff --check`, redaction scan, paired benchmark report, dependency/provenance review и documented retention/disable/recovery procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol/compactor/policy versions, test IDs, fixture/config hashes, measured/estimated outcomes, recovery/fallback counts and redaction status. Не включать raw prompts/outputs/source corpus, secrets, credentials, absolute paths, PII or provider payloads.

После подтверждения criteria перенести фактический contract в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать rollback: отключение content compaction оставляет обычный Context Budget path работоспособным; failed/security/no-benefit outcome не объявляется savings и не повторяет external effect вслепую.

После полного закрытия направления удалить комплект `123-0`…`123-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, compactor/policy versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, retention/rollback/recovery procedure записаны.

## Связанный issue

- [issue #103](https://github.com/rkfsociety/EvoHime/issues/103)
