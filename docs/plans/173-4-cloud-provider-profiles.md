# План 173.4 — Cloud Provider Profiles: verification и закрытие

## Зависимости

### Блокирующие

- [IPC/UI projection](./173-3-cloud-provider-profiles.md).
- GitHub CI workflow, module-router/release gates и canonical docs.

### Опциональные

- Developer-only live smoke tests с credentials; они не входят в обычный CI.

## Реализация

- Добавить unit/contract tests для всех built-in profiles, config parsing,
  catalog normalization, capability provenance, deduplication, stale/recovery,
  401/429/model removal/protocol drift, cancellation и security redaction.
- Добавить mock HTTP fixtures для compatible/native paths; проверить, что
  provider metadata не меняет permission, approval, privacy или execution
  policy и что secrets не попадают в logs/artifacts/UI.
- Провести static/protocol/typecheck и разрешённые узкие локальные проверки;
  полный acceptance оставить GitHub CI согласно проектным правилам. Сохранить
  только фактически полученное CI evidence.
- Перенести подтверждённые contract/state/evidence в
  `docs/architecture.md`, `docs/current-state.md`, `docs/release-evidence.md`
  и `docs/development-plan.md`, проверить ссылки и удалить `173-*.md`.

## Критерии

- Все критерии обзора 173 подтверждены кодом и свежими CI evidence.
- `git diff --check` проходит, module/release routing соответствует затронутым
  исходникам, task-only commit содержит только изменения плана/реализации.
- Issue #105 удаляется только после сохранённого и привязанного плана; до
  реализации этот план не объявляется завершённой функциональностью.

## Non-goals

Признание provider availability или free tier подтверждёнными без эмпирической
проверки из плана 174.
