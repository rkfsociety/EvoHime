# План 120.4 — Grounded Research Workspace: verification, release-evidence и закрытие

Статус: этап 4 для [плана 120.0](./120-0-grounded-research-workspace.md); после [плана 120.3](./120-3-grounded-research-workspace.md).

## Цель

Подтвердить требования issue #100 свежими воспроизводимыми тестами, оформить redacted release evidence и перенести фактически реализованный contract в canonical documentation.

## Зависимости

### Блокирующие

- План 120.3 — полный Core/ingestion/retrieval/research/IPC/UI vertical slice.
- `docs/architecture.md`, `docs/current-state.md`, `docs/development-plan.md`, release-evidence и security/eval gates.

### Опциональные

- Semantic Map, optional embeddings/OCR, browser reading mode, ContextRefs, export and Diagnostics Bundle.

## Матрица проверки

- text/Markdown/workspace/uploaded/PDF text-layer/ProjectArtifact/web/connector ingestion, immutable revision/hash, parser version, unsupported scanned PDF and failed-source isolation;
- same revision/index reuse, new revision isolation, lexical/semantic/structural/metadata retrieval, exact human locator and stale/deleted/root-denied behavior;
- source policies including SelectedOnly no-web, selected+web snapshot-before-citation, trust/sensitivity/local-only boundaries and no prompt-injection authority;
- Quick/Deep/Comparison sessions, bounded decomposition/subtopics, cancellation/restart, budget/source-limited coverage and last-good artifact recovery;
- valid/wrong/missing locator, quote mismatch, multi-source claim, contradiction preservation, inference/unsupported/weak/stale states and no factual-certainty overclaim;
- immutable ResearchArtifact, ProjectArtifact promotion, source drift, rerun, ResearchDelta and old artifact pinning;
- IPC auth/bounds/replay/resync/idempotency/stale outcomes, renderer provenance forgery rejection and metadata-only UI;
- no secrets, raw prompts/outputs, unrestricted source corpus, connector credentials, absolute paths or PII in storage/logs/IPC/release bundle.

## Обязательные gates

1. Focused Core/storage/ingestion/retrieval/research/citation/recovery/security tests with migration backup, rollback and fault injection.
2. `cargo fmt --all -- --check`, relevant `cargo clippy --locked ... -- -D warnings`, affected Rust tests и desktop IPC tests.
3. Electron `npm run check:protocol`, `npm run typecheck`, focused research tests, `npm test`, production build/bundle checks и native package smoke при изменении packaging.
4. `git diff --check`, redaction scan, dependency/provenance review и documented disable/retention/recovery procedure.

## Release-evidence и закрытие

Evidence bundle содержит commit, schema/protocol/parser/index versions, test IDs, source/artifact/delta hashes, typed outcomes, coverage/omission summary and redaction status. Не включать credentials, raw corpus, prompts/outputs, PII or provider payloads.

После подтверждения criteria перенести фактический contract в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates в `docs/development-plan.md`/`docs/release-evidence.md`. Зафиксировать rollback: отключение research pipeline не ломает existing RAG/artifact/browser paths; failed acquisition/citation не объявляется success и не подменяет source.

После полного закрытия направления удалить комплект `120-0`…`120-4` согласно `docs/plans/README.md`; до этого stage-файлы сохраняются для трассировки.

## Definition of Done

- [ ] Все criteria issue подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты либо есть явно принятый typed degradation.
- [ ] Ссылки, schema/tags, parser/index versions и фактические пути сверены с checkout.
- [ ] Release bundle redacted, retention/rollback/recovery procedure записаны.

## Связанный issue

- [issue #100](https://github.com/rkfsociety/EvoHime/issues/100)
