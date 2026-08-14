# План: Context Budget Manager

Статус: draft для ревью.

## Цель

Сделать управление контекстом Core явным и измеримым: Ева должна перед каждым
вызовом модели выбирать нужные инструкции, память, историю, результаты tools и
рабочие заметки в пределах bounded token budget.

## Границы

Владелец состояния и политики — Rust Core. Electron только показывает состав
контекста и причины сокращения. HTTP, внешний prompt-service и перенос runtime
состояния в renderer не добавляются.

В план входят:

- scratchpad текущей задачи;
- оценка размера контекста и budget reserve для ответа/tool-call;
- сжатие истории и больших tool outputs;
- pruning устаревших, дублирующихся и конфликтующих сведений;
- выбор ограниченного tool loadout по намерению задачи;
- безопасная телеметрия selection/compression/isolation.

Не входят: изменение IPC major-версии без необходимости, автономное удаление
пользовательской памяти и обязательная vector database.

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

### 1. Контракт и измерение

- Ввести Core-owned `ContextBudget` с лимитами system, user, memory, tools,
  history, scratchpad и output reserve.
- Уточнить единую оценку токенов; число должно быть доступно в
  `ModelContext` и trace.
- Ввести `ContextItem` с `id`, `kind`, `source`, `priority`, `bytes`,
  `estimated_tokens`, `privacy`, `selected` и `drop_reason`.
- Не сохранять в diagnostics сырой prompt или тело памяти; сохранять ids,
  counts, hashes и policy labels.

### 2. Scratchpad и offload

- Добавить task-scoped scratchpad в SQLite или в bounded task state.
- Разделить заметки на facts, open_questions, decisions, tool_findings и
  next_actions.
- Большие результаты filesystem/search оставлять во внешнем task artifact, а в
  контекст помещать краткое резюме, hash и locator.
- После перезапуска Core восстанавливать только подтверждённый scratchpad;
  незавершённые записи помечать `recovered/unverified`.

### 3. Compression и pruning

- Перед моделью удалять дубликаты, старые tool outputs и записи с меньшим
  приоритетом.
- При превышении soft limit запускать bounded summarizer; при недоступной
  модели использовать детерминированное усечение по границам сообщений.
- Конфликтующие инструкции не склеивать: новую запись помещать выше старой и
  явно маркировать override.
- Сохранять связь `summary_id -> source_ids`, чтобы результат можно было
  проверить и повторно построить.

### 4. Tool loadout

- Разделить инструменты на обязательные, read-only и mutation groups.
- Сначала использовать детерминированный intent router; semantic selection
  добавлять только после появления evaluation fixtures.
- Передавать модели только небольшой релевантный набор schemas, сохраняя
  полный registry в Core.
- Никогда не скрывать permission/approval semantics у выбранного инструмента.

### 5. IPC и UI

- Расширить read-only `ModelContext` additive-полями: budget, selected item
  ids, dropped item ids/reasons, compression summary и tool loadout.
- Добавить команду просмотра scratchpad только через Core и с bounded output.
- Дать UI действия `summarize now`, `clear task scratchpad` и `forget memory`
  через существующие approval/privacy правила.

## Проверки

- unit-тесты budget arithmetic, deterministic ordering и конфликтного pruning;
- property-тесты на bounded output и отсутствие отрицательных budgets;
- Core integration: большой tool output → offload → summary → replay;
- IPC compatibility tests для additive-полей;
- E2E: один и тот же prompt даёт одинаковый loadout при одинаковом registry;
- security test: diagnostics не содержат prompt, memory body, secret или raw
  tool result.

## Критерии готовности

- каждый model call имеет bounded budget и объяснимый ledger selection;
- task context переживает restart без восстановления непроверенных фактов;
- oversized history не ломает задачу и не вызывает неограниченное усечение;
- tool schemas передаются только по loadout, но Core всё равно проверяет вызов;
- UI честно показывает, что было выбрано, сжато или отброшено.

## Зависимости и порядок

Сначала нужен Memory extraction/conflicts для приоритетов памяти, затем Local
Agentic RAG для источников. Evaluation catalog должен появиться до semantic
tool selection. Signed receipts не блокируют этот план, но должны получать
context ledger hash для аудита.
