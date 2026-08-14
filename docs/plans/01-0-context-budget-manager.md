# План 01: Context Budget Manager

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Сделать управление контекстом Core явным и измеримым: Ева должна перед каждым
вызовом модели выбирать нужные инструкции, память, историю, результаты tools и
рабочие заметки в пределах bounded token budget.

## Границы

Владелец состояния и политики — Rust Core. Electron получает только bounded
read-only projection состава контекста и причин сокращения. HTTP, внешний
prompt-service и перенос runtime-состояния в renderer не добавляются.

В план входят:

- scratchpad текущей задачи;
- оценка размера контекста и budget reserve для ответа/tool-call;
- сжатие истории и больших tool outputs;
- pruning устаревших, дублирующихся и конфликтующих сведений;
- выбор ограниченного tool loadout по намерению задачи;
- безопасная телеметрия selection/compression/isolation.

Не входят: изменение IPC major-версии без необходимости, автономное удаление
пользовательской памяти и обязательная vector database. Первый рабочий путь не
зависит от RAG или semantic selection.

## Целевой контур

```text
user prompt
   -> ContextPlanner
   -> select instructions + memories + scratchpad + tools
   -> compress/offload oversized inputs
   -> ModelContext event + model call
   -> update scratchpad and context ledger
```

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 01.1 | [Контракт и измерение](01-1-budget-contract-and-measurement.md) | `ContextBudget`, `ModelContextProfile`, tokenizer/estimator, `ContextItem`, `context_ledger` и его hash | 02.5, 03.1, 04.3, 05.3 |
| 01.2 | [Scratchpad и offload](01-2-scratchpad-and-offload.md) | task artifact store и bounded scratchpad | 05.3 |
| 01.3 | [Compression и pruning](01-3-compression-and-pruning.md) | — внутренний этап | — |
| 01.4 | [Tool loadout](01-4-tool-loadout.md) | — внутренний этап | — |
| 01.5 | [IPC и UI](01-5-context-ipc-and-ui.md) | additive-поля `ModelContext`, Core-команды scratchpad/forget | UI |

Порядок: 01.1 первым, остальные — в любом порядке после него. 01.3 и 01.4
ничего не отдают наружу и не блокируют другие планы.

## Зависимости плана

Блокирующих зависимостей от других планов нет: это фундамент, от которого
зависят остальные. Нужны только существующие Core, SQLite и model gateway.

Опциональные интеграции, не блокирующие этот план:

- реализованный Memory Extraction (см. [`../architecture.md`](../architecture.md))
  — источник записей памяти для selection и каскадного `forget memory`; до
  подключения память в контексте ограничивается Memory v1;
- Local Agentic RAG (план 02) — поставщик evidence blocks; до его появления
  контекст собирается без документных цитат;
- semantic tool selection разрешается только после evaluation catalog,
  deterministic intent router работает без него.

Ключевое следствие: весь внешний контракт, кроме artifact store, сосредоточен
в этапе 01.1, поэтому планы 02, 03 и 04 разблокируются после одного этапа, а
не после всего плана.
