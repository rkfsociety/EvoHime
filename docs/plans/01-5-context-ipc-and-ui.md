# Этап 01.5: IPC и UI

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

## Зависимости

Блокирующие: этап 01.1 (budget и selected/dropped items), 01.2 (scratchpad для
команд просмотра и очистки), 01.3 (compression summary в projection), 01.4
(tool loadout в projection).

Это последний этап плана: он показывает результат остальных.

## Что этап отдаёт наружу

Additive-поля read-only `ModelContext` и Core-команды просмотра scratchpad,
`summarize now`, `clear task scratchpad` и `forget memory`.

## Содержание

- Расширить read-only `ModelContext` additive-полями: `schema_version`, budget,
  selected item ids, bounded dropped item ids/reasons, compression summary и
  tool loadout. Старые Electron/WinUI clients игнорируют неизвестные поля;
  добавить compatibility tests для старой и новой схемы без major bump.
- Добавить команду просмотра scratchpad только через Core и с bounded output.
- Дать UI действия `summarize now`, `clear task scratchpad` и `forget memory`
  только через Core, существующие approval/privacy rules, rate limit и audit.
  Каждая команда является mutation и получает ledger entry. `forget memory`
  каскадно удаляет производные summaries, scratchpad links и task artifacts,
  сохраняя redacted audit факт удаления.
- UI показывает человекочитаемые bounded причины и влияние операции, но не
  получает тела памяти, raw tool outputs или неограниченный список ids.

## Проверки

- IPC compatibility tests для additive-полей: старый client игнорирует
  неизвестные поля, major bump не нужен;
- `forget memory` каскадно удаляет производные summaries, scratchpad links и
  task artifacts, оставляя redacted audit факт удаления;
- каждая mutation-команда получает ledger entry и подчиняется rate limit;
- security test: UI не получает prompt, memory body, secret, raw tool result и
  неограниченный список ids.

## Критерии готовности

- UI честно показывает, что было выбрано, сжато или отброшено, с bounded
  человекочитаемыми причинами;
- каскадное `forget memory` не оставляет производных данных;
- privacy и approval соблюдаются для всех новых команд.
