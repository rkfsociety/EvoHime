# План 176.4 — Guided Capability Recipes: verification и закрытие

## Зависимости

### Блокирующие

- [IPC, guided UI и fork](./176-3-guided-capability-recipes.md).
- Existing workflow/eval/RAG/tool/approval tests, security policy, GitHub CI,
  module-router and canonical docs.

### Опциональные

- Deterministic provider/model fixtures for optional live integrations.

## Реализация

- Добавить tests для catalog/version immutability, preflight states, eight
  built-ins, workflow binding/grant subset, approval/denial, structured output,
  RAG provenance and prompt-injection isolation.
- Проверить model/prompt comparison uses common fixtures and benchmark owner;
  local-fit recipe consumes hardware/capability evidence; exact reproduction
  rejects unavailable revisions; fork strips secrets and preserves provenance.
- Проверить cancellation/restart, missing adapters, bounded IPC/UI, redaction,
  raw prompt/response absence and no automatic publication. Local checks only
  narrowly; full acceptance through CI with actual evidence recorded.
- Перенести contract/state/evidence в `docs/architecture.md`,
  `docs/current-state.md`, `docs/release-evidence.md` и
  `docs/development-plan.md`, проверить links и удалить `176-*.md` after real
  implementation closure.

## Критерии

- Все критерии обзора 176 подтверждены кодом и CI evidence.
- `git diff --check` проходит, task-only commit contains only related changes,
  historical issue #154 is not a closure gate.

## Non-goals

Считать recipe documentation или model self-score доказательством успешного
runtime/quality результата.
