# План 113.4 — Policy-Aware Tool Result Cache: freshness, provenance и safe reuse read-only calls: verification, release-evidence и закрытие

Статус: этап 4 для [плана 113.0](./113-0-policy-aware-tool-result-cache.md); после [плана 113.3](./113-3-policy-aware-tool-result-cache.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 113.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Tools/actions имеют explicit trusted cacheability metadata.
- [ ] Default cacheability = Never.
- [ ] Cache key учитывает version/schema/resource/account/policy context.
- [ ] Есть TTL/freshness и explicit `RequireFresh`.
- [ ] Cached results сохраняют source provenance/observed time.
- [ ] Mutating tools не используют result cache в MVP.
- [ ] Workspace/provider/credential drift инвалидирует entries.
- [ ] Sensitive cache storage регулируется policy.
- [ ] Есть bounded storage/eviction и optional single-flight.

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

- [issue](https://github.com/rkfsociety/EvoHime/issues/93)
