# Этап 01.2: Scratchpad и offload

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

## Зависимости

Блокирующие: этап 01.1 — записи scratchpad являются `ContextItem` и попадают в
ledger.

Разблокирует: 05.3 (context isolation ребёнка и offload больших результатов).

## Что этап отдаёт наружу

Task artifact store и bounded task-scoped scratchpad, а также Core-операции над
ними, поверх которых 01.5 строит команды UI: bounded чтение scratchpad с
фильтром по категории и `status`, очистка task-scoped scratchpad, удаление
записи вместе с её производными ссылками в artifact store и чтение полного
артефакта по locator с повторной policy/approval-проверкой. Каждая операция —
mutation с ledger entry, кроме двух операций чтения.

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
- `status` и `drop_reason` — разные измерения: `draft` не восстанавливается,
  `confirmed` может попасть в контекст, а `recovered` всегда получает
  `trust=unverified`, пониженный priority и при selection отображается как
  `drop_reason=unverified`, если не был явно подтверждён заново. Автоматическое
  подтверждение выключено по умолчанию; безопасные read-only категории могут
  включаться отдельной Core policy.
- После restart восстанавливать в рабочий контекст только `confirmed`. Записи
  `recovered/unverified` изолировать в recovery view, понижать их priority,
  не использовать как instructions и автоматически удалять по умолчанию через
  1 час или после 10 шагов (меньшее из условий); значения являются Core policy
  и могут быть изменены пользователем. Перезапись подтверждённой записи допускается только новой ревизией
  с conflict entry, а не silent override.
- Scratchpad имеет жёсткий лимит размера в пределах своей категории бюджета.
  При превышении Core автоматически выгружает самые старые `confirmed` записи
  в artifact store, оставляя в контексте bounded summary с hash и locator;
  минимально обязательный контекст и `open_questions` текущего шага при этом не
  вытесняются. Молчаливое усечение записи запрещено.
- Большие результаты filesystem/search хранить в локальном Core-owned task
  artifact store, не во внешнем сервисе. В контекст помещать bounded summary,
  hash, locator, размер, TTL и privacy label; полный результат позже читать
  отдельным Core API с повторной policy/approval-проверкой.
- Artifact store дедуплицирует содержимое по `content_hash` из 01.1: повторный
  offload того же содержимого переиспользует существующий артефакт и добавляет
  ссылку, а не копию. Store имеет bounded квоту на задачу и на диск; вытеснение
  идёт по TTL и последнему обращению, при этом артефакт, на который ссылается
  живой ledger entry или confirmed запись scratchpad, не удаляется молча —
  ссылка помечается как `expired` с сохранением hash и размера. После удаления
  содержимого hash сохраняется как tombstone только для аудита и не считается
  доступным dedup-hit для нового offload.
- При чтении артефакта по locator Core заново считает `content_hash` и
  сравнивает с сохранённым. Расхождение означает повреждение или подмену:
  содержимое не попадает в контекст, ссылка помечается `invalid`, а вызов
  продолжается без него с явной причиной в ledger.
- Artifact store общий на уровне Core, но пространство имён — per-task:
  дедупликация по `content_hash` может переиспользовать содержимое между
  задачами, а вот доступ по locator ограничен задачей-владельцем и её детьми
  (05.3), с наследованием privacy label. Квота считается и на задачу, и на диск
  целиком. Запись артефакта и обновление ссылок на него атомарны; конкурентный
  offload одинакового содержимого из двух задач даёт один артефакт и две
  ссылки, а не гонку.
- Контракт store версионируется вместе с этапом: 01.3 и 05.3 зависят от
  описанного здесь поведения (dedup, TTL, вытеснение, tombstone, проверка
  hash при чтении), а не от конкретной реализации хранилища.
- Внешние tool outputs считать недоверенными данными, помещать в отдельный
  `data_not_instructions` envelope и проверять на prompt-injection перед
  извлечением scratchpad. Текст внутри envelope не разбирается как policy,
  даже если имитирует system-инструкцию; содержимое tool output не может менять
  policy, permissions, approval или system instructions.

## Проверки

- SQLite crash/recovery во время записи scratchpad, TTL, immutable revisions и
  concurrent ledger updates;
- большой tool output попадает в artifact store, а в контекст — только summary
  с hash и locator;
- повторный offload одинакового содержимого не создаёт второй артефакт, а
  переполнение квоты вытесняет по TTL/последнему обращению без молчаливой
  потери ссылок из ledger;
- переполненный scratchpad выгружает старые `confirmed` записи в artifact store
  и не усекает записи молча;
- после restart в рабочий контекст возвращаются только `confirmed` записи, а
  `recovered/unverified` остаются изолированными;
- security test: внешние данные не становятся instructions и не меняют policy,
  permissions или approval.

## Критерии готовности

- task context переживает restart без восстановления непроверенных фактов;
- ledger и метрики показывают offloaded bytes и recovery outcome;
- concurrent writes и crash/restart не нарушают immutable revisions.
