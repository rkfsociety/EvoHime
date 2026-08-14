# Этап 01.1: Контракт и измерение

Этап плана [01 Context Budget Manager](01-0-context-budget-manager.md).

**Это следующий шаг работы по всему каталогу планов.** Он ничего не ждёт и
разблокирует планы 03, 04 и 05.

## Зависимости

Блокирующие: только существующие Core, SQLite и model gateway.

Разблокирует: 02.5 (запись причины выбора evidence в ledger), 03.1
(`context_ledger_hash` в payload receipt), 04.3 (budget/profile snapshot для
route decision) и 05.3 (budget ребёнка).

## Что этап отдаёт наружу

`ContextBudget`, `ModelContextProfile`, versioned tokenizer/estimator,
`ContextItem` со справочником `drop_reason` и `context_ledger` вместе с его
hash через versioned Core event/API.

## Содержание

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
- Отказ провайдера по длине контекста считать ошибкой оценки, а не поводом
  расширить бюджет: Core выполняет ровно один deterministic re-plan с
  ужесточённым профилем и той же обязательной частью контекста. Повторный отказ
  завершает вызов через `BudgetUnavailable`; каскад re-plan запрещён.
- Ввести `ContextItem` с `id`, `task_id`, `session_id`, `parent_id`, `kind`,
  `source`, `priority`, `trust`, `privacy`, `created_at`, `last_used_at`,
  `ttl`, `retention`, `pinned`, `version`, `tokenizer_version`, `content_hash`,
  `bytes`, `estimated_tokens`, `selected` и `drop_reason`.
- `content_hash` считается по нормализованному содержимому item и служит
  единым основанием для дедупликации, `drop_reason=duplicate`, conflict
  detection и дедупликации artifact store.
- `pinned` выставляется только пользовательской командой, поднимает эффективный
  priority и выводит item из обычного pruning. Pin не может нарушить
  `hard_limit`, вытеснить минимально обязательный контекст или удержать item с
  истёкшим retention: при нехватке бюджета pinned item отбрасывается последним
  и с явным `drop_reason`.
- Зафиксировать справочник `drop_reason`: `over_budget`, `low_priority`,
  `duplicate`, `superseded`, `expired`, `unverified`, `offloaded`,
  `privacy_restricted`, `invalid_tool_state` и `policy_denied`.
- Зафиксировать покрытие `context_ledger_hash`: он считается по ids выбранных
  item, их порядку в собранном контексте, версиям profile и tokenizer и по
  применённым compression/pruning-решениям. Одинаковый hash обязан означать
  одинаковый фактический вход модели; изменение порядка или решения о сжатии
  меняет hash.
- Не сохранять в diagnostics сырой prompt, тело памяти или raw tool output;
  сохранять только ids, counts, hashes, policy labels, bounded reasons,
  `compression_ratio`, `offloaded_bytes` и budget counters.

## Проверки

- unit-тесты budget arithmetic, model profiles, tokenizer overhead и
  deterministic ordering;
- property-тесты: budget никогда не превышается, минимально обязательный
  контекст и permissions сохраняются, output остаётся bounded даже при ошибке
  estimator;
- профиль неизвестной модели не может обойти обязательный минимум: несовместимые
  значения дают `BudgetUnavailable`, а не молчаливое превышение;
- ledger hash меняется при изменении порядка item, версии profile/tokenizer или
  применённого compression-решения и совпадает при идентичном входе модели;
- context-length error провайдера даёт ровно один re-plan, затем
  `BudgetUnavailable`; каскад re-plan не возникает;
- pinned item не вытесняет минимально обязательный контекст и не приводит к
  превышению `hard_limit`;
- security tests: diagnostics не содержат prompt, memory body, secret или raw
  tool result.

## Критерии готовности

- каждый model call имеет bounded budget и объяснимый ledger selection;
- ledger и метрики показывают dropped percentage и budget violations;
- `context_ledger_hash` доступен другим планам через versioned Core event/API.
