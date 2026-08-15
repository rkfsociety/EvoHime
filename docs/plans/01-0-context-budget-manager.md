# План 01: Context Budget Manager

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Сделать управление контекстом Core явным и измеримым: Ева должна перед каждым
вызовом модели выбирать нужные инструкции, память, историю, результаты tools и
рабочие заметки задачи (scratchpad) в пределах bounded token budget. «Рабочие
заметки» и «scratchpad» — один и тот же объект из 01.2, отдельной сущности нет.

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

При нехватке бюджета сокращение идёт по иерархии прав, а не по свежести:
safety/hard-deny и approval policy > system instructions > явные ограничения
пользователя > confirmed task decisions/facts > history/tool data >
recovered/unverified. Recency и trust работают только как тай-брейк внутри
одного уровня. Полные правила и conflict detection — в 01.3, справочник
`drop_reason` и поведение `pinned` — в 01.1.

## Целевой контур

```text
user prompt
   -> ContextPlanner
   -> select instructions + memories + scratchpad + tools
   -> compress/offload oversized inputs
   -> final budget validation (mandatory + selected + reserves <= hard_limit)
   -> ModelContext event + model call
   -> update scratchpad and context ledger
```

Шаг final budget validation обязателен и выполняется после compress/offload, до
формирования `ModelContext` event: при невыполнении условия Core повторяет
разрешённые deterministic drops, а после их исчерпания завершает вызов через
`BudgetUnavailable` (01.1).

`ContextPlanner` здесь — внутренний компонент Core, а не элемент внешнего
контракта: его интерфейс может меняться свободно. Наружу отдаётся только то,
что перечислено в колонке «Что отдаёт наружу» ниже.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 01.1 | [Контракт и измерение](01-1-budget-contract-and-measurement.md) | `ContextBudget`, `ModelContextProfile`, tokenizer/estimator, `ContextItem`, `context_ledger` и его hash | 02.5, 03.1, 04.3, 05.3 |
| 01.2 | [Scratchpad и offload](01-2-scratchpad-and-offload.md) | task artifact store и bounded scratchpad | 01.3, 01.5, 05.3 |
| 01.3 | [Compression и pruning](01-3-compression-and-pruning.md) | внутренний: наружу уходят только записи ledger из 01.1 (`drop_reason`, `summary_id -> source_ids`, compression-решения) | 01.5 |
| 01.4 | [Tool loadout](01-4-tool-loadout.md) | внутренний: наружу уходят только loadout id, intent и `loadout_miss` diagnostic через ledger 01.1 | 01.5 |
| 01.5 | [IPC и UI](01-5-context-ipc-and-ui.md) | additive-поля `ModelContext`, Core-команды scratchpad/`summarize now`/`clear task scratchpad`/`forget memory`/`pin/unpin item` | UI |

Порядок: 01.1 обязателен первым; 01.2 и 01.4 можно выполнять параллельно после
него; 01.3 начинается после 01.2, потому что использует artifact store; 01.5
завершает план после 01.2–01.4. Частичный порядок имеет вид
`01.1 -> (01.2, 01.4) -> 01.3 -> 01.5`.

01.3 не зависит от результатов 01.4: сжатие и pruning работают над уже
собранным набором item и ничего не знают о выборе tool schemas. Поэтому после
завершения 01.2 этапы 01.3 и 01.4 допустимо вести параллельно; единственная
жёсткая цепочка — `01.1 -> 01.2 -> 01.3`.

Зависимость 01.3 → 01.5 жёсткая: `ModelContext` из 01.5 обязан содержать
compression summary и bounded причины сокращения, а их источник появляется
только в 01.3. Собственного канала между внутренними этапами и UI нет: 01.3 и
01.4 пишут решения в `context_ledger` из 01.1, а 01.5 читает оттуда bounded
projection. Начинать 01.5 раньше допустимо только как заготовку схемы, но этап
не считается готовым без полей compression/loadout.

Artifact store определён в 01.2 (жизненный цикл, дедупликация по `content_hash`
из 01.1, квоты, вытеснение по TTL/последнему обращению, tombstone и отдельный
Core API для чтения полного содержимого). 01.3 и 05.3 используют именно это
определение, отдельного контракта store в 01.1 нет.

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

Ключевое следствие: контракты budget/profile/ledger из 01.1 разблокируют планы
02, 03 и 04, artifact store из 01.2 нужен плану 05.3, а UI/IPC-контракт не
считается готовым до 01.5.
