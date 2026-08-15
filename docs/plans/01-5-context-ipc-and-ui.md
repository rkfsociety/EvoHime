# Этап 01.5: IPC и UI

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

## Зависимости

Блокирующие: этап 01.1 (budget и selected/dropped items), 01.2 (scratchpad для
команд просмотра и очистки), 01.3 (compression summary в projection), 01.4
(tool loadout в projection).

Это последний этап плана: он показывает результат остальных.

## Что этап отдаёт наружу

Additive-поля read-only `ModelContext` и Core-команды просмотра scratchpad,
`summarize now`, `clear task scratchpad`, `forget memory` и `pin/unpin item`.

## Содержание

- Расширить read-only `ModelContext` additive-полями: `schema_version`, budget,
  selected item ids, bounded dropped item ids/reasons, `context_ledger_hash`,
  compression summary и tool loadout. Старые Electron/WinUI clients игнорируют
  неизвестные поля;
  добавить compatibility tests для старой и новой схемы без major bump.
- Добавить команду просмотра scratchpad только через Core и с bounded output.
- Дать UI действия `summarize now`, `clear task scratchpad`, `forget memory` и
  `pin/unpin item` только через Core, существующие approval/privacy rules, rate
  limit и audit. Каждая команда является mutation и получает ledger entry.
  `forget memory` каскадно удаляет производные summaries, scratchpad links и
  task artifacts, сохраняя redacted audit факт удаления.
- `summarize now` действует только на текущую task-scoped сборку контекста и
  не меняет долговременную память без отдельной policy-операции.
- `pin/unpin item` выставляет флаг `pinned` из 01.1. UI показывает, что pin
  повышает приоритет, но не гарантирует включение в контекст: при нехватке
  бюджета pinned item отбрасывается последним и с явной причиной.
- До реализации Memory Extraction каскад `forget memory` ограничен записями
  Memory v1 и их производными; UI не обещает удаление того, что Core ещё не
  умеет отслеживать. Memory v1 здесь — существующий механизм памяти Core с
  явно созданными пользователем записями, без автоматического извлечения фактов
  из диалога и без графа производных сущностей.
- Отказ сборки контекста показывать явно: `BudgetUnavailable` доходит до UI как
  bounded локализованная причина с кодом, `required_tokens`,
  `available_tokens`, profile version и указанием, какой минимум не поместился,
  а не как молчаливый обрыв ответа или generic error. Сырые prompt и содержимое
  памяти в причине не передаются.
- UI показывает человекочитаемые bounded причины и влияние операции, но не
  получает тела памяти, raw tool outputs или неограниченный список ids.

## Проверки

- IPC compatibility tests для additive-полей: старый client игнорирует
  неизвестные поля, major bump не нужен;
- `forget memory` каскадно удаляет производные summaries, scratchpad links и
  task artifacts, оставляя redacted audit факт удаления;
- каждая mutation-команда получает ledger entry и подчиняется rate limit;
- `BudgetUnavailable` доходит до UI bounded причиной, а не молчаливым обрывом
  или generic error;
- pinned item не отменяет hard limit: при нехватке бюджета UI получает явную
  причину его отбрасывания;
- security test: UI не получает prompt, memory body, secret, raw tool result и
  неограниченный список ids.

## Критерии готовности

- UI честно показывает, что было выбрано, сжато или отброшено, с bounded
  человекочитаемыми причинами;
- каскадное `forget memory` не оставляет производных данных;
- privacy и approval соблюдаются для всех новых команд.
