# План 55.4 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой: verification, release-evidence и закрытие

Статус: этап 4 для [плана 55.0](./55-0-agentic-browser-session.md); после [плана 55.3](./55-3-agentic-browser-session.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 55.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Есть Core-owned BrowserSession lifecycle.
- [ ] Модель работает через typed browser tools и stable element refs.
- [ ] Refs имеют page revision и stale protection.
- [ ] Есть network/SSRF policy с private-address protection.
- [ ] Default browser profile isolated/ephemeral.
- [ ] Upload/download проходят Artifact/Core boundaries.
- [ ] Workbench может показывать безопасную live projection.
- [ ] Human takeover поддерживается без гонки с agent actions.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Runtime/recovery/fault-injection tests для cancel, stale, denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal, escalation, secret leakage и untrusted input.
6. Redirect/DNS-rebinding/private-IP tests, stale element refs, isolated profile
   cleanup, ArtifactStore upload/download and human-takeover race tests; prove
   legacy raw CDP/CSS paths are not registered as bypasses.
7. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

8. Проверить package-manifest/fixture наличия packaged browser backend и
   lifecycle cleanup; отдельно доказать, что `EVOHIME_BROWSER_CDP_URL`, raw
   CSS selector и прямой screenshot workspace path не принимаются production
   router-ом.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `55-0` … `55-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.
- [ ] Evidence matrix содержит exact schema/IPC tags, backend/package hash,
  fixture IDs, commands и фактические результаты; гипотезы о performance не
  выдаются за измерения.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/35)
