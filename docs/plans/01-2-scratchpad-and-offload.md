# Этап 01.2: Scratchpad и offload

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

## Зависимости

Блокирующие: этап 01.1 — записи scratchpad являются `ContextItem` и попадают в
ledger.

Разблокирует: 05.3 (context isolation ребёнка и offload больших результатов).

## Что этап отдаёт наружу

Task artifact store и bounded task-scoped scratchpad.

## Содержание

- Добавить task-scoped scratchpad в SQLite или в bounded task state.
- Разделить заметки на facts, open_questions, decisions, tool_findings и
  next_actions.
- Каждая запись получает `status=draft|confirmed|recovered`, `trust`,
  `revision`, `parent_id`, TTL и immutable ledger entry. Подтверждённой
  считается только атомарно записанная Core-запись, созданная после
  provenance/policy-проверки результата инструмента, явного пользовательского
  подтверждения или завершённой policy-операции; успешный tool result и
  заметка модели сами по себе не становятся фактом.
- После restart восстанавливать в рабочий контекст только `confirmed`. Записи
  `recovered/unverified` изолировать в recovery view, понижать их priority,
  не использовать как instructions и автоматически удалять по TTL либо после
  N шагов. Перезапись подтверждённой записи допускается только новой ревизией
  с conflict entry, а не silent override.
- Большие результаты filesystem/search хранить в локальном Core-owned task
  artifact store, не во внешнем сервисе. В контекст помещать bounded summary,
  hash, locator, размер, TTL и privacy label; полный результат позже читать
  отдельным Core API с повторной policy/approval-проверкой.
- Внешние tool outputs считать недоверенными данными, явно маркировать их как
  `data_not_instructions` и проверять на prompt-injection перед извлечением
  scratchpad. Содержимое tool output не может менять policy, permissions,
  approval или system instructions.

## Проверки

- SQLite crash/recovery во время записи scratchpad, TTL, immutable revisions и
  concurrent ledger updates;
- большой tool output попадает в artifact store, а в контекст — только summary
  с hash и locator;
- после restart в рабочий контекст возвращаются только `confirmed` записи, а
  `recovered/unverified` остаются изолированными;
- security test: внешние данные не становятся instructions и не меняют policy,
  permissions или approval.

## Критерии готовности

- task context переживает restart без восстановления непроверенных фактов;
- ledger и метрики показывают offloaded bytes и recovery outcome;
- concurrent writes и crash/restart не нарушают immutable revisions.
