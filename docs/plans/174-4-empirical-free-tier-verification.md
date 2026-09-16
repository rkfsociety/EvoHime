# План 174.4 — Free Access Evidence: verification и закрытие

## Зависимости

### Блокирующие

- [Routing, IPC и UI](./174-3-empirical-free-tier-verification.md).
- GitHub CI, module-router/release workflow и canonical documentation.

### Опциональные

- Credential-gated live smoke tests; обычный CI использует deterministic mock
  fixtures и не требует provider accounts.

## Реализация

- Добавить contract tests для states/allowances/activation, semantic response
  validation, credits-vs-tokens, account/region/model scope, stale/TTL,
  402/403/429 invalidation, quota budget, cancellation, restart fencing,
  routing reasons и privacy redaction.
- Проверить integration с #125 без дублирования gateway/circuit/reliability и
  что `FreeOnly` не вызывает paid route при отсутствии evidence.
- Запустить только необходимые быстрые локальные проверки и полный acceptance
  через GitHub CI согласно текущим правилам; зафиксировать фактические gates,
  не выдавая live provider claims без evidence.
- Перенести подтверждённый contract/state/release evidence в
  `docs/architecture.md`, `docs/current-state.md`, `docs/release-evidence.md`
  и `docs/development-plan.md`, проверить ссылки и удалить `174-*.md`.

## Критерии

- Все критерии обзора 174 подтверждены кодом и CI evidence.
- `git diff --check` проходит, task-only commit содержит только относящиеся к
  задаче файлы.
- Issue #152 удаляется только после сохранения этих связанных plan files.

## Non-goals

Объявлять любой catalog label доказательством текущей бесплатности или
обещать постоянные provider quotas.
