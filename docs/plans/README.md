# Планы реализации

Каждый файл — один этап, доводимый до рабочего состояния и ревьюемый отдельно.
Имя файла `NN-M-slug.md`: `NN` — план, `M` — этап внутри него, `M = 0` — обзор
плана. Список файлов в алфавитном порядке и есть порядок реализации.

## С чего начинать

Планы 01–06 реализованы целиком и удалены из каталога: их контракты живут в
[`../architecture.md`](../architecture.md), подтверждённое состояние — в
[`../current-state.md`](../current-state.md). В каталоге остаются одиннадцать
незавершённых направлений: сначала план 07, затем планы 08, 09, 10, 11,
12, 13, 14, 15, 16 и 17.
Их обзоры:
[`07-0-superagi-inspired-tooling.md`](07-0-superagi-inspired-tooling.md),
[`08-0-execution-ledger.md`](08-0-execution-ledger.md),
[`09-0-policy-and-capabilities.md`](09-0-policy-and-capabilities.md),
[`10-0-ipc-adapters-and-providers.md`](10-0-ipc-adapters-and-providers.md),
[`11-0-memory-and-rag.md`](11-0-memory-and-rag.md),
[`12-0-telemetry-and-evaluation.md`](12-0-telemetry-and-evaluation.md) и
[`13-0-browser-backend.md`](13-0-browser-backend.md) и
[`14-0-voice-and-ambient-audio.md`](14-0-voice-and-ambient-audio.md),
[`15-0-vision-and-documents.md`](15-0-vision-and-documents.md),
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
| 07 SuperAGI-inspired tool manifests, Action Console и telemetry | предложен; обзор в [`07-0-superagi-inspired-tooling.md`](07-0-superagi-inspired-tooling.md) | 07-0 блокируется реализованным workflow-контрактом из [`../architecture.md`](../architecture.md); 07-1 от 07-0; 07-2 от 07-1; 07-3 от 07-1; 07-4 от 07-1 и 07-3 |
| 08 Core-owned execution ledger и typed receipts | проектируется; обзор в [`08-0-execution-ledger.md`](08-0-execution-ledger.md) | 08-1 от текущих EventJournal/receipts/IPC; 08-2 от 08-1; 08-3 от 08-2; 08-4 от 08-3 |
| 09 Policy, capabilities и approval | проектируется; обзор в [`09-0-policy-and-capabilities.md`](09-0-policy-and-capabilities.md) | 09-1 от плана 08 и текущей policy; 09-2 от 09-1; 09-3 от 09-2; 09-4 от 09-3 |
| 10 IPC, version negotiation и provider boundary | проектируется; обзор в [`10-0-ipc-adapters-and-providers.md`](10-0-ipc-adapters-and-providers.md) | 10-1 от планов 08–09 и текущего IPC; 10-2 от 10-1; 10-3 от 10-2; 10-4 от 10-3 |
| 11 Typed memory и Core-first RAG | проектируется; обзор в [`11-0-memory-and-rag.md`](11-0-memory-and-rag.md) | 11-1 от планов 08–10 и текущего RAG; 11-2 от 11-1; 11-3 от 11-2; 11-4 от 11-3 |
| 12 Local telemetry и deterministic evaluation | проектируется; обзор в [`12-0-telemetry-and-evaluation.md`](12-0-telemetry-and-evaluation.md) | 12-1 от планов 08–11 и текущего event/evaluation harness; 12-2 от 12-1; 12-3 от 12-2; 12-4 от 12-3 |
| 13 Изолированный browser backend | проектируется; обзор в [`13-0-browser-backend.md`](13-0-browser-backend.md) | 13-1 от планов 08–12 и tool-runtime; 13-2 от 13-1; 13-3 от 13-2; 13-4 от 13-3 |
| 14 Voice pipeline и ambient audio | проектируется; обзор в [`14-0-voice-and-ambient-audio.md`](14-0-voice-and-ambient-audio.md) | 14-1 от планов 08–12 и listener; 14-2 от 14-1; 14-3 от 14-2; 14-4 от 14-3 |
| 15 Vision и document worker | проектируется; обзор в [`15-0-vision-and-documents.md`](15-0-vision-and-documents.md) | 15-1 от планов 08–12 и решения о worker; 15-2 от 15-1; 15-3 от 15-2; 15-4 от 15-3 |
| 16 Workflow, automation и simulation | проектируется; обзор в [`16-0-workflow-automation-and-simulation.md`](16-0-workflow-automation-and-simulation.md) | 16-1 от планов 08–12 и существующих workflow contracts; 16-2 от 16-1; 16-3 от 16-2; 16-4 от 16-3 |
| 17 Общие release gates и нерешённые решения | сопровождающий план; обзор в [`17-0-release-criteria-and-open-decisions.md`](17-0-release-criteria-and-open-decisions.md) | 17-1 от планов 07–16; 17-2 от 17-1; 17-3 от 17-2; 17-4 от 17-3 |

Порядок незавершённых этапов задаётся так: сначала последовательно выполняется
07-1 → 07-2 → 07-3 → 07-4, затем 08-1 → 08-2 → 08-3 → 08-4, затем 09-1 → 09-2 → 09-3 → 09-4, затем
10-1 → 10-2 → 10-3 → 10-4, затем 11-1 → 11-2 → 11-3 → 11-4, затем
12-1 → 12-2 → 12-3 → 12-4, затем 13-1 → 13-2 → 13-3 → 13-4, затем
14-1 → 14-2 → 14-3 → 14-4, затем 15-1 → 15-2 → 15-3 → 15-4, затем
16-1 → 16-2 → 16-3 → 16-4 и 17-1 → 17-2 → 17-3 → 17-4. Обзоры 07-0, 08-0, 09-0, 10-0, 11-0, 12-0, 13-0, 14-0, 15-0, 16-0 и 17-0 не являются исполняемыми этапами;
они фиксируют границы и граф зависимостей соответствующего плана. План 07-4
может использовать общий deterministic evaluation harness
(`crates/evohime-core/src/evals.rs`, `tests/evals/`) как опциональную
зависимость, но не блокирует его отсутствие.

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

## Что здесь не хранится

Статус реализации. Подтверждённое состояние checkout живёт в
[`../current-state.md`](../current-state.md), исполняемый цикл — в
[`../development-plan.md`](../development-plan.md), долгосрочные направления — в
[`../roadmap.md`](../roadmap.md).
