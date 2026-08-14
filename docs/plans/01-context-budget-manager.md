# План: Context Budget Manager

Статус: draft для ревью.

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

### 1. Контракт и измерение

- Ввести Core-owned `ContextBudget` с уровнями `target`, `soft_limit` и
  `hard_limit` для system, user, memory, tools, history, scratchpad и output.
  Ни один model call не отправляется после `hard_limit`.
- Минимально обязательными считать safety/system policy, approval и permission
  semantics, текущий user prompt, активное состояние tool-call и cancellation
  context. Их budget резервируется до обычного pruning; если профиль модели не
  позволяет вместить обязательный минимум и резервы, Core завершает вызов
  bounded `BudgetUnavailable`, а не нарушает hard limit.
- Ввести versioned `ModelContextProfile`, выбираемый по provider/model. Профиль
  обязан содержать `max_context_tokens`, `target_tokens`, `soft_limit_tokens`,
  `hard_limit_tokens`, `tool_schema_reserve`, `tool_call_reserve`,
  `final_answer_reserve`, `streaming_reserve` и `retry_reserve`. Базовый
  fallback-профиль: `target=60%`, `soft_limit=75%`, `hard_limit=85%` от
  заявленного окна модели, минимум 1024 токена под tool-call и 2048 под
  final answer; значения проверяются на совместимость с обязательным минимумом,
  неизвестная модель не может обойти эти ограничения.
- `target` расходуется на контекст, а резервы считаются отдельно и не могут
  быть заняты history или schemas. Профиль и фактическое распределение
  сохраняются в ledger; provider usage после ответа обновляет диагностику.
- Зафиксировать model-specific tokenizer/estimator: имя, версию, chat-template,
  tool-schema overhead, метаданные и правило округления. Оценка deterministic и
  versioned, кэшируется для неизменных item. При расхождении с фактическим
  usage Core применяет консервативный over-estimate, помечает событие и
  корректирует профиль, но не расширяет уже начатый запуск.
- Ввести `ContextItem` с `id`, `task_id`, `session_id`, `parent_id`, `kind`,
  `source`, `priority`, `trust`, `privacy`, `created_at`, `last_used_at`,
  `ttl`, `retention`, `version`, `tokenizer_version`, `bytes`,
  `estimated_tokens`, `selected` и `drop_reason`.
- Зафиксировать справочник `drop_reason`: `over_budget`, `low_priority`,
  `duplicate`, `superseded`, `expired`, `unverified`, `offloaded`,
  `privacy_restricted`, `invalid_tool_state` и `policy_denied`.
- Не сохранять в diagnostics сырой prompt, тело памяти или raw tool output;
  сохранять только ids, counts, hashes, policy labels, bounded reasons,
  `compression_ratio`, `offloaded_bytes` и budget counters.

### 2. Scratchpad и offload

- Добавить task-scoped scratchpad в SQLite или в bounded task state.
- Разделить заметки на facts, open_questions, decisions, tool_findings и
  next_actions.
- Каждая запись получает `status=draft|confirmed|recovered`, `trust`,
  `revision`, `parent_id`, TTL и immutable ledger entry. Подтверждённой
  считается только атомарно записанная Core-запись, созданная после
  provenance/policy-проверки результата инструмента, явного пользовательского
  подтверждения или завершённой policy-операции; успешный tool result и
  заметка модели сами по себе не становятся фактом.
- После restart восстанавливать в рабочий контекст только `confirmed`. Записи
  `recovered/unverified` изолировать в recovery view, понижать их priority,
  не использовать как instructions и автоматически удалять по TTL либо после
  N шагов. Перезапись подтверждённой записи допускается только новой ревизией
  с conflict entry, а не silent override.
- Большие результаты filesystem/search хранить в локальном Core-owned task
  artifact store, не во внешнем сервисе. В контекст помещать bounded summary,
  hash, locator, размер, TTL и privacy label; полный результат позже читать
  отдельным Core API с повторной policy/approval-проверкой.
- Внешние tool outputs считать недоверенными данными, явно маркировать их как
  `data_not_instructions` и проверять на prompt-injection перед извлечением
  scratchpad. Содержимое tool output не может менять policy, permissions,
  approval или system instructions.

### 3. Compression и pruning

- Перед моделью удалять дубликаты, старые tool outputs и записи с меньшим
  приоритетом.
- При превышении `soft_limit` запускать отдельный bounded summarizer с
  собственным `summary_budget`, входным лимитом и запретом tool calls/retries.
  Если summarizer недоступен, превышает свой бюджет или возвращает invalid
  output, применять deterministic fallback без каскадного повторного запуска.
- Fallback удаляет сначала expired/duplicate/low-priority items, затем самые
  старые tool outputs, сохраняя system/policy/approval/user constraints,
  подтверждённые факты, числа, пути, отрицания и валидные пары tool-call/result.
  Нельзя резать середину сообщения или нарушать состояние незавершённого
  tool-call.
- Original items остаются source of truth в ledger/artifact store. Summary —
  только projection для текущего model call; сохранять связь
  `summary_id -> source_ids`, tokenizer/profile versions и возможность
  повторной сборки.
- Для system/instructions действует иерархия прав, а не простая recency:
  safety/hard-deny и approval policy > system instructions > явные ограничения
  пользователя > confirmed task decisions/facts > history/tool data >
  recovered/unverified. Новая запись не может понизить более высокий уровень.
  Для facts применять conflict detection и label `conflicting`, а не silent
  override; при существенном конфликте нужен пользовательский confirmation.

### 4. Tool loadout

- Разделить инструменты на обязательные, read-only и mutation groups.
- Сначала использовать детерминированный intent router; semantic selection
  добавлять только после появления evaluation fixtures.
- Обязательные инструменты всегда входят в loadout и имеют отдельный
  `mandatory_schema_reserve`; при неопределённом intent использовать безопасный
  read-only fallback loadout.
- Передавать модели только небольшой релевантный набор schemas с лимитом
  `tool_schema_reserve`, сохраняя полный registry в Core. Semantic selection
  разрешать только после evaluation catalog и измерений precision/recall.
- Никогда не скрывать permission/approval semantics у выбранного инструмента.
- Вызов инструмента вне loadout Core отклоняет до эффекта с bounded diagnostic
  `loadout_miss`; автоматический fallback разрешён только для явно разрешённой
  read-only замены.

### 5. IPC и UI

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

- unit-тесты budget arithmetic, model profiles, tokenizer overhead,
  deterministic ordering и конфликтного pruning;
- property-тесты: budget никогда не превышается, минимальный контекст и
  permissions сохраняются, output остаётся bounded даже при ошибке estimator;
- Core integration: большой tool output → offload → summary → replay;
- SQLite crash/recovery во время записи scratchpad, TTL, immutable revisions и
  concurrent ledger updates;
- IPC compatibility tests для additive-полей;
- E2E фиксирует prompt, model/profile/tokenizer versions, registry snapshot,
  memory snapshot и timestamp policy; одинаковый fixture даёт одинаковый
  loadout без flaky-зависимости от wall clock;
- тесты сохранения чисел, путей, отрицаний, policy/permission rules и валидного
  tool-call state после compression;
- load tests для длинной истории и большого числа tool calls;
- security tests: diagnostics/UI не содержат prompt, memory body, secret или
  raw tool result; внешние данные не становятся instructions; out-of-loadout
  mutation блокируется.

## Критерии готовности

- каждый model call имеет bounded budget и объяснимый ledger selection;
- task context переживает restart без восстановления непроверенных фактов;
- oversized history не ломает задачу, не вызывает неограниченное усечение и
  сохраняет минимально обязательный контекст;
- tool schemas передаются только по loadout, но Core всё равно проверяет вызов;
- UI честно показывает, что было выбрано, сжато или отброшено, с bounded
  человекочитаемыми причинами;
- ledger и метрики показывают dropped percentage, compression quality/ratio,
  loadout misses, budget violations, offloaded bytes и recovery outcome;
- criteria включают crash/restart, concurrent writes, privacy, approval и
  каскадное forget memory.

## Зависимости

Блокирующих зависимостей от других планов нет: это фундамент, от которого
зависят остальные. Нужны только существующие Core, SQLite и model gateway.

Опциональные интеграции, не блокирующие этот план:

- Memory Extraction (план 02) — источник записей памяти для selection и
  каскадного `forget memory`; до его появления память в контексте
  ограничивается Memory v1;
- Local Agentic RAG (план 03) — поставщик evidence blocks; до его появления
  контекст собирается без документных цитат;
- semantic tool selection разрешается только после evaluation catalog,
  deterministic intent router работает без него.

Что этот план обязан предоставить другим:

- `context_ledger` и его hash через versioned Core event/API — их требуют
  Signed receipts (план 04);
- bounded интерфейс отбора evidence — его требует Local Agentic RAG (план 03);
- budget/profile snapshot — его требуют SLM routing (план 05) и child
  workflows (план 06).

Порядок внутри плана: сначала deterministic MVP (budget/profile/token
estimator, scratchpad, recovery, deterministic pruning, tool loadout), затем
внешние интеграции через явные bounded интерфейсы.
