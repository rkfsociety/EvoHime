# Планы реализации

Каждый файл — один этап, доводимый до рабочего состояния и ревьюемый отдельно.
Имя файла `NN-M-slug.md`: `NN` — план, `M` — этап внутри него, `M = 0` — обзор
плана. Список файлов в алфавитном порядке и есть порядок реализации.

## С чего начинать

Планы 01–06, 08 и 09 реализованы целиком и удалены из каталога: их контракты
живут в [`../architecture.md`](../architecture.md), подтверждённое состояние —
в [`../current-state.md`](../current-state.md). В каталоге остаются шесть
незавершённых направлений: сначала план 07, затем планы 16 и 17.
Их обзоры:
[`16-0-workflow-automation-and-simulation.md`](16-0-workflow-automation-and-simulation.md) и
[`17-0-release-criteria-and-open-decisions.md`](17-0-release-criteria-and-open-decisions.md).

## Правило нумерации

**Этап может блокирующе зависеть только от этапов с меньшим номером плана либо
от более ранних этапов своего плана.**

Из этого следуют два требования к каждому файлу этапа:

1. Секция «Зависимости» разделяет **блокирующие** (без них этап невыполним) и
   **опциональные** (без них этап выполним, но часть возможностей деградирует
   предсказуемым и явно описанным образом).
2. Опциональная зависимость обязана описывать поведение до её появления.
   Формулировка вида «для X нужен план N» без такого описания запрещена: именно
   она превращает рабочий этап в кажущийся заблокированным.

Циклы недопустимы. Взаимная связь двух планов разрешается тем, что ровно одна
её сторона объявляется блокирующей, а вторая — опциональной с описанной
деградацией.

## Планы и порядок

| План | Обзор | Блокирующие зависимости |
| --- | --- | --- |
| 01 Signed hash-chain receipts | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 02 Локальный SLM fallback и routing | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 03 Специализированные child workflows | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 04 Постоянное слушание и ambient-память | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 05 Provenance и реконструируемость model request | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 06 CAMEL/AutoGPT-inspired workflow orchestration для Евы | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 07 SuperAGI-inspired tool manifests, Action Console и telemetry | предложен; исполняемый обзор пока не добавлен | отдельное направление |
| 08 Core-owned execution ledger и typed receipts | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 09 Policy, capabilities и approval | реализован; контракт перенесён в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 10 IPC, version negotiation и provider boundary | реализован; контракт перенесён в [`../architecture.md`](../architecture.md), состояние — в [`../current-state.md`](../current-state.md) | — |
| 11 Typed memory и Core-first RAG | реализован; контракт перенесён в [`../architecture.md`](../architecture.md), состояние — в [`../current-state.md`](../current-state.md) | — |
| 12 Local telemetry и deterministic evaluation | реализован; контракт перенесён в [`../architecture.md`](../architecture.md), состояние — в [`../current-state.md`](../current-state.md) | — |
| 16 Workflow, automation и simulation | 16.4 реализован; acceptance fixtures и границы перенесены в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md) | — |
| 17 Общие release gates и нерешённые решения | 17.4 audit завершён; технические gates PASS, release BLOCKED по open decisions в [`../release-audit.md`](../release-audit.md) | — |

Порядок незавершённых этапов задаётся так: сначала последовательно выполняется
16-1 → 16-2 → 16-3 → 16-4 и 17-1 → 17-2 → 17-3 → 17-4. Обзоры 16-0 и 17-0 не являются исполняемыми этапами;
они фиксируют границы и граф зависимостей соответствующего плана.

## Что уже реализовано

Реализованные планы удаляются из каталога, а не помечаются галочкой: их
подтверждённое состояние живёт в [`../current-state.md`](../current-state.md),
а контракт — в [`../architecture.md`](../architecture.md).

Так уже удалены планы Memory Extraction (коммиты `0d67554`, `4b376c6`), Context
Budget Manager, Local Agentic RAG, план 04 Постоянное слушание и ambient-память
и план 01 Signed hash-chain receipts целиком (этапы 01.1 Canonical contract, 01.2 Key lifecycle,
01.3 Runtime integration, 01.4 Chain storage и export). Их контракты живут в
[`../architecture.md`](../architecture.md), подтверждённое состояние — в
[`../current-state.md`](../current-state.md),
`../security/receipt-canonical-v1.md` и
`../security/receipt-key-lifecycle-v1.md`.

Удаляется только этап, выполненный **целиком**: критерии готовности достигнуты
и покрыты тестами. Частично сделанное не удаляется, а описывается в секции
«Что уже есть в коде» внутри самого этапа — с обеих сторон: что есть и чего
нет. Существующий модуль, который никем не вызывается, считается отсутствующим
поведением: библиотека без подключения не закрывает этап.

Проверка на 2026-08-21: план 01 реализован целиком, включая 01.4 (SQLite
receipts_v1, signed checkpoints, retention/compaction, verified_pruned,
chain-aware offline verifier, ListReceipts/VerifyReceipts/ExportReceipts IPC).
Планы 02 и 03 тоже реализованы целиком и удалены из каталога по правилу выше,
поэтому упоминания «следующий незавершённый этап — 02.1» здесь больше нет.
План 04 реализован целиком (этапы 04.1–04.7) и удалён из каталога по тому же
правилу. План 05 реализован целиком: контракт, durable storage, Core integration,
evidence/shadowing, receipts и tool linkage, recovery, redaction/retention и
offline verify/export перенесены в архитектуру и current-state.

Проверка на 2026-08-23: план 08 Core-owned execution ledger и typed receipts
реализован целиком (этапы 08-1–08-4): versioned typed contract, atomic
SQLite storage поверх schema v30, startup reconciliation, IPC-проекция с
typed `ReplayGap`/`FullSnapshot` и дедупликацией по `event_id`, реальная
production-линковка `ToolCall`/`Observation`/`ToolReceipt` с подписанным
`receipts_v1` и всех трёх исходов approval (approve/reject/expiry).
Единственная задокументированная граница — `dispatch_terminal_execute` не
имеет живого cancellation-триггера (пробел, существовавший до плана 08, не
расширение полномочий этой задачей); сам ledger-контракт для
`Cancelling`/`Cancelled` полностью реализован и протестирован на
storage/IPC уровне. Контракт перенесён в архитектуру и current-state.

## Что здесь не хранится

Статус реализации. Подтверждённое состояние checkout живёт в
[`../current-state.md`](../current-state.md), исполняемый цикл — в
[`../development-plan.md`](../development-plan.md), долгосрочные направления — в
[`../roadmap.md`](../roadmap.md).
