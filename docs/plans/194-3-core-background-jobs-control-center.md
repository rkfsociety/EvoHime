# План 194.3 — IPC и Background Jobs UI

Статус: active implementation contract. Этап следует после 194.1–194.2.

## Scope и изменяемые контракты
1. Расширить authenticated IPC command 262/event 107 additive полями или отдельными read/action commands только при необходимости. Сохранить proto compatibility, sequence replay и bounded frame size.
2. Добавить в Electron вкладку/раздел «Фоновые задачи» с фильтрами по status, owner и origin; default view показывает active/waiting items.
3. Карточка задачи показывает тип, owner, статус, наблюдаемый этап, progress evidence, next wake/wait reason, timestamps, dependencies, attempts summary и artifact/evidence links.
4. Отображать actions только когда Core сообщил capability/precondition: отмена, просмотр результата, переход к исходной задаче. Не показывать декоративные кнопки для неподдерживаемых действий.
5. Добавить detail view с bounded attempt history и typed error details. Raw logs, prompts, job inputs и credentials не передавать в renderer.
6. Сохранить responsive/accessibility behavior, keyboard navigation, empty/loading/unavailable/stale/partial/error/reconnect states.
7. Reconnect получает snapshot и продолжает events без дубликатов; renderer не становится владельцем state.

## Зависимости
### Блокирующие
194.1–194.2; authenticated IPC, generated TypeScript bindings и existing Electron UI navigation/state patterns.
### Опциональные
План 193 coordinator/child links; OperationsPanel cards.

## Recovery и rollback
Старый shell продолжает работу с прежней проекцией. New UI скрывается при typed unavailable, а не отображает пустой список. Reconnect reloads snapshot when cursor is stale. Rollback удаляет UI navigation/commands; Core lifecycle не затрагивается.

## Verification
- npm run check:protocol.
- Renderer tests for filters, ordering, pagination, status labels, empty, unavailable, stale, error, reconnect and duplicate events.
- Accessibility tests for keyboard focus, labels and status announcements.
- Contract tests for bounded projection and absence of sensitive fields.
- UI tests ensure controls render only for allowed capabilities.

## Release evidence
IPC tags/schema, renderer test matrix, accessibility results, projection bounds/redaction tests and focused CI links.

## Критерии выхода
- [ ] Пользователь видит состояние detached jobs без открытия исходной задачи.
- [ ] Все displayed actions работают и имеют Core authorization.
- [ ] UI не выдумывает прогресс и не скрывает unknown/partial failures.
- [ ] check:protocol и focused renderer tests проходят.
