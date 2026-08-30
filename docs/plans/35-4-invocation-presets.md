# План 35.4 — Invocation Presets: version-pinned шаблоны запусков без копирования секретов: verification, release-evidence и закрытие

Статус: этап 4 для [плана 35.0](./35-0-invocation-presets.md); после [плана 35.3](./35-3-invocation-presets.md).

## Цель

Доказать все требования, оформить redacted evidence и принять решение implemented либо blocked с причиной.

## Зависимости

### Блокирующие

- План 35.3 — Core, IPC и client projection.
- Project release-evidence, documentation и security/eval gates.

### Опциональные

- Дополнительные планы из overview только расширяют matrix после базовых критериев.

## Матрица критериев

- [ ] `C01` — Есть durable InvocationPreset contract.
- [ ] `C02` — Preset pinned к workflow version.
- [ ] `C03` — Можно создать preset из completed run.
- [ ] `C04` — Credentials хранятся только как refs.
- [ ] `C05` — Secret inputs не сохраняются raw по умолчанию.
- [ ] `C06` — Есть migration flow между workflow versions.
- [ ] `C07` — Preset запускается через обычный workflow runtime.
- [ ] `C08` — Preset можно использовать scheduler без обхода approvals.
- [ ] `C09` — Preset можно создать вручную из workflow detail.
- [ ] `C10` — Удалённый/expired credential даёт `NeedsRebinding`.
- [ ] `C11` — Временный override не изменяет сохранённую revision.
- [ ] `C12` — Schedule фиксирует revision/hash snapshot.
- [ ] `C13` — Version drift показывает preview и не выполняет silent migration.
- [ ] `C14` — Trigger base mapping optional и не может переопределить protected identities.

## Обязательная проверка

1. Unit/contract tests для schema, hash, transitions, bounds и errors.
2. Storage/migration tests для backup, rollback, idempotency и corruption.
3. Runtime/recovery/fault-injection tests для cancel, stale, denial, restart и unknown outcome.
4. IPC/adapter/renderer or CLI tests для auth, redaction, replay/resync и optimistic conflict.
5. Security/eval tests по фактическим критериям направления: secret leakage,
   credential rebinding, protected capability/approval fields, schema drift,
   untrusted input и bounded sensitive projection.
6. Проверить cargo fmt --all -- --check, релевантный cargo clippy -D warnings, npm run check:protocol, npm run typecheck, npm test и git diff --check.

## Release-evidence и закрытие

- Bundle содержит commit, versions, test IDs, hashes, typed outcomes и redaction status; credentials, raw output, transcripts, absolute paths и PII исключены.
- Evidence отдельно подтверждает completed-run sanitizer, manual-create
  validation, migration compatible/incompatible outcomes, NeedsRebinding,
  temporary override isolation и schedule revision/hash snapshot.
- Отдельным evidence должен быть automation↔workflow scheduler adapter:
  schedule create/edit drift, immutable preset reference, обычный approval
  path, restart и duplicate polling; текущие automation scheduler unit tests
  без preset reference этот критерий не закрывают.
- Rollback/disable и recovery procedure записаны; unknown side effect не объявляется success и не повторяется вслепую.
- После свежих проверок обновить docs/architecture.md, docs/current-state.md, docs/development-plan.md и при необходимости docs/release-evidence.md.
- После закрытия всего направления и переноса подтверждённого контракта удалить комплект `35-0` … `35-4`; отдельные stage-файлы до этого не удалять. Незавершённое направление оставить blocked с evidence.

## Definition of Done

- [ ] Все критерии матрицы подтверждены reproducible evidence.
- [ ] Blocking dependencies закрыты.
- [ ] Ссылки и версии соответствуют checkout.
- [ ] Release bundle redacted.

## Связанный issue

- [issue](https://github.com/rkfsociety/EvoHime/issues/15)
