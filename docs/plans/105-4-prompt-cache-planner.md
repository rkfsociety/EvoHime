# План 105.4 — Prompt Cache Planner: stable context segments, provider-aware cache hints и reuse metrics: verification, release-evidence и закрытие

Статус: этап 4 для [плана 105.0](./105-0-prompt-cache-planner.md); после [плана 105.3](./105-3-prompt-cache-planner.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 105.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Model context имеет explicit hashed PromptSegments.
- [ ] Stable/dynamic segments сериализуются deterministic.
- [ ] Provider cache capabilities описаны через profile, не provider-name branches.
- [ ] Exact revisions/policy versions участвуют в invalidation.
- [ ] Cache plan не меняет instruction/security semantics.
- [ ] Usage умеет показывать measured cache metrics.
- [ ] Keepalive отсутствует по умолчанию и bounded при явном включении.
- [ ] Benchmark/evals покрывают cache reuse/invalidation.

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

- [issue](https://github.com/rkfsociety/EvoHime/issues/85)
