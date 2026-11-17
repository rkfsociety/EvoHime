# План 196.3 — durable-совместимость, IPC и UI-проекции

Статус: active implementation contract. Этап следует после 196.2.

## Scope и изменяемые контракты
1. Подтвердить и закрепить тестами, что схема SQLite не меняется: колонки
   `run_tool_metrics.tool_name`, `tool_intents.tool_name`,
   `receipt_actions.tool_name`, `receipt_audit_markers.tool_name` остаются
   TEXT. Новых миграций нет; меняется только содержимое новых записей.
2. Receipts: `runtime_signing.rs` (read-only семплер) и
   `runtime_projection::sampled_read_only` принимают обе формы при чтении
   исторических цепочек; `runtime_payload` продолжает подписывать сохранённую
   строку без перезаписи.
3. Проверить `ipc_bridge_terminal_review.rs`: литерал
   `builtin:shell_execute:v1` заменяется каноническим именем, хеш
   пересчитывается осознанно, а не случайно.
4. IPC proto (`evohime.desktop.proto`) не меняет типы полей
   `ReconcilePendingReceiptAction.tool_name`, `SkillMetadata.allowed_tools`,
   `required_tools`: старый shell-клиент присылает точки, Core резолвит их
   этапом 2. Изменение требует согласованной правки обеих сторон и
   `npm run check:protocol`.
5. UI-словарь: `tool-names.ts` (24 записи) и `chat-summary.ts` получают
   канонические id и нормализуют разделитель при чтении, поэтому история чатов
   с точечными именами показывает подпись, а не сырой id.
6. Имена IPC-событий (`git.status`, `git.diff`, `memory.search`) и команд
   Electron (`workflow.start`, `workflow.cancel`) не переименовываются: это
   отдельный лексикон, и смена сломала бы протокол и историю событий.
7. Approval binding: `canonical_call_hash(tool_name, scope, input)` хеширует
   имя, поэтому одобрение, выданное до смены словаря, не совпадает — это
   допустимо, потому что approvals краткоживущие; повторное одобрение
   запрашивается на канонический id.

## Зависимости
### Блокирующие
196.1 и 196.2; `evohime-receipts`; `desktop-ipc` и генерируемые типы;
Electron renderer tests.
### Опциональные
План 194 (панель background jobs) и 195 (extension catalog) показывают id в UI;
их подписи обновляются без изменения поведения.

## Recovery и rollback
Исторические данные не мигрируются и не перезаписываются: чтение
поддерживает обе формы. Неизвестная строка в истории не ломает проекцию — она
показывается как есть. Rollback UI-словаря возвращает fallback-подпись, но не
влияет на Core.

## Verification
- Receipt replay: цепочка, записанная до переименования, проходит сверку.
- Test: `sampled_read_only` считает read-only только канонический id новой
  записи; историческая строка сохраняет собственное значение.
- `npm run check:protocol` и Electron vitest-набор зелёные.
- Test: история чата с событием `git.status` отображается корректной подписью.

## Release evidence
Результаты `check:protocol`, Electron test summary, replay отчёт по
исторической receipt-цепочке и подтверждение отсутствия новых миграций.

## Критерии выхода
- [ ] Новая запись содержит только канонический id.
- [ ] Старая запись читается и подписывается как прежде.
- [ ] IPC совместим в пределах major-версии.
- [ ] UI показывает подписи для обеих форм.
