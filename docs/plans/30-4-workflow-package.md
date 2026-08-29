# План 30.4 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей: verification, release-evidence и закрытие

Статус: этап 4 для [плана 30.0](./30-0-workflow-package.md); после [плана 30.3](./30-3-workflow-package.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 30.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] Есть versioned package format.
- [ ] Export удаляет credentials/secrets/runtime-specific state.
- [ ] Есть dependency manifest.
- [ ] Import выполняет validate/resolve/preview до записи.
- [ ] Credential slots требуют локального rebinding.
- [ ] Сохраняется безопасная provenance/fork lineage.
- [ ] Canonical hash позволяет duplicate/diff detection.
- [ ] Import не расширяет Core capability registry.
- [ ] До explicit commit import не пишет workflow/version, не создаёт
  schedule/trigger и не запускает workflow.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Package runtime/recovery tests для preview без записи, atomic commit,
   duplicate hash, restart, malformed/oversized input и unknown commit outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: traversal,
   executable asset rejection, capability escalation, secret/schema redaction,
   credential-value leakage, unknown capability registration и untrusted input.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

Отдельные issue cases должны быть видимы в evidence: round-trip и
deterministic export, stripped credential/schema/runtime/webhook ids,
dependency missing/version/schema report, duplicate hash, provenance/fork,
malformed/oversized package, traversal, unknown capability, no workflow or
schedule/trigger start, opaque credential rebinding и old-format migration
или typed rejection.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `30-0` … `30-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/10)
