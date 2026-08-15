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

- Ввести Core-owned `ContextBudget` с уровнями `target_tokens`, `soft_limit_tokens` и
  `hard_limit_tokens` для system, user, memory, tools, history, scratchpad и
  output. Ни один model call не отправляется после `hard_limit_tokens`.
- Минимальный обязательный набор (`minimum_viable_context`) вычислять
  детерминированно из текущего
  состояния Core: всегда включать safety/system policy и текущий user prompt;
  добавлять approval/permission semantics при наличии операции, активное
  состояние незавершённого tool-call и cancellation context при наличии этих
  состояний. Набор и его причины фиксировать в ledger до обычного pruning.
  Если профиль модели не позволяет вместить обязательный минимум и резервы,
  Core завершает вызов bounded `BudgetUnavailable` сразу, до запуска selection
  и drops, а не нарушает hard limit. Safety- и approval-часть входит в
  `minimum_viable_context` и не сокращается никогда: конфликт «safety не влезает
  в бюджет» разрешается отказом от вызова, а не урезанием safety.
- Зафиксировать контракт `BudgetUnavailable` как терминальный результат сборки
  контекста, а не как исключение внутри неё. Поля: `code`, `stage`
  (`mandatory_overflow` | `drops_exhausted` | `provider_replan_failed` |
  `estimator_unavailable`), `required_tokens`, `available_tokens`,
  `profile_version`, `tokenizer_version`, `context_ledger_hash` частичной
  сборки и bounded `missing_part` — какая именно категория обязательного набора
  не поместилась. Model call при этом не выполняется, а ledger получает запись
  с этим же кодом. Автоматический retry запрещён на всех уровнях: повторная
  попытка возможна только новым пользовательским действием или сменой профиля.
  `ModelContext` для такого шага формируется с уже известными полями и флагом
  отказа, чтобы UI (01.5) и дочерние задачи (05.1, 05.3) отличали его от прочих
  ошибок Core и не трактовали как обрыв соединения.
- Ввести versioned `ModelContextProfile`, выбираемый по provider/model. Профиль
  обязан содержать `max_context_tokens`, `target_tokens`, `soft_limit_tokens`,
  `hard_limit_tokens`, `tool_schema_reserve`, `tool_call_reserve`,
  `final_answer_reserve`, `streaming_reserve` и `retry_reserve`. Базовый
  fallback-профиль: `target_tokens=60%`, `soft_limit_tokens=75%`,
  `hard_limit_tokens=85%` от
  заявленного окна модели, минимум 1024 токена под tool-call и 2048 под
  final answer; значения проверяются на совместимость с обязательным минимумом,
  а сумма обязательного минимума и всех резервов не может превышать
  `hard_limit_tokens`. Неизвестная модель использует fallback-профиль и не
  может обойти эти ограничения.
- `target_tokens` расходуется на контекст, а резервы считаются отдельно и не могут
  быть заняты history или schemas. Профиль и фактическое распределение
  сохраняются в ledger; provider usage после ответа обновляет диагностику.
- Финальная проверка выполняется после selection, compression и fallback:
  `mandatory_tokens + selected_optional_tokens + reserves <= hard_limit_tokens`.
  Если условие не выполняется, Core переходит к следующему уровню лестницы
  сокращения, а не повторяет тот же уровень. Лестница конечна, упорядочена и
  задана заранее: expired/duplicate → low-priority optional → самые старые tool
  outputs → offload крупных item в artifact store → сжатие истории → отказ от
  необязательных резервов в фиксированном порядке. Каждый уровень применяется
  не более одного раза за сборку и обязан строго уменьшать
  `selected_optional_tokens`; уровень, не давший уменьшения, считается
  исчерпанным немедленно. Число итераций ограничено длиной лестницы, поэтому
  цикл завершается всегда. После последнего уровня Core возвращает
  `BudgetUnavailable` со `stage=drops_exhausted` без model call.
- Зафиксировать правила версионирования контрактов этапа. `ContextBudget`,
  `ModelContextProfile`, `ContextItem` и `context_ledger` версионируются
  независимо друг от друга целым `schema_version`. Добавление необязательного
  поля и расширение справочника `drop_reason` — minor-изменение: потребители
  обязаны игнорировать неизвестные поля и трактовать неизвестный `drop_reason`
  как `unknown` без ошибки. Удаление или переименование поля, смена семантики
  существующего и сужение справочника требуют нового major и одновременного
  обновления планов 02.5, 03.1, 04.3 и 05.3. Версии tokenizer, нормализатора и
  стратегии входят в `context_ledger_hash`, поэтому их изменение всегда меняет
  hash и не считается совместимым.
- Зафиксировать допустимую погрешность оценки: estimator обязан быть
  консервативным, то есть его оценка не ниже фактического usage провайдера.
  Целевая относительная погрешность на верхней стороне — не более 5% от
  фактических prompt tokens; превышение фиксируется как diagnostic
  `estimator_drift` и корректирует профиль для следующих вызовов. Занижение
  оценки считается дефектом: погрешность вниз не допускается вообще, а
  однократное занижение обрабатывается как context-length error с deterministic
  re-plan по правилу ниже.
- Зафиксировать model-specific tokenizer/estimator: имя, версию, chat-template,
  tool-schema overhead, метаданные и правило округления. Оценка deterministic и
  versioned, кэшируется для неизменных item. При расхождении с фактическим
  usage Core применяет консервативный over-estimate, помечает событие и
  корректирует профиль, но не расширяет уже начатый запуск.
- Отказ провайдера по длине контекста считать ошибкой оценки, а не поводом
  расширить бюджет: Core выполняет ровно один deterministic re-plan с профилем
  версии `v+1`, где `hard_limit_tokens` уменьшается до `min(provider_window,
  floor(previous_hard_limit * 0.9))`, а необязательные резервы сокращаются по
  заранее заданному порядку. Обязательная часть контекста не меняется. Повторный
  отказ завершает вызов через `BudgetUnavailable`; каскад re-plan запрещён.
- Ввести `ContextItem` с `id`, `task_id`, `session_id`, `parent_id`, `kind`,
  `source`, `priority`, `trust`, `privacy`, `created_at`, `last_used_at`,
  `ttl`, `retention`, `pinned`, `version`, `tokenizer_version`, `content_hash`,
  `bytes`, `estimated_tokens`, `selected` и `drop_reason`.
- `content_hash` считается по нормализованному содержимому item и служит
  единым основанием для дедупликации, `drop_reason=duplicate`, conflict
  detection и дедупликации artifact store. Нормализация versioned: UTF-8,
  Unicode NFC, нормализация переводов строк и завершающих пробелов; для
  структурированных JSON-данных — канонический порядок ключей и фиксированное
  представление чисел. Версия нормализатора входит в hash input.
- `pinned` выставляется только пользовательской командой, поднимает эффективный
  priority и выводит item из обычного pruning. Pin не может нарушить
  `hard_limit`, вытеснить минимально обязательный контекст или удержать item с
  истёкшим retention: при нехватке бюджета pinned item отбрасывается последним
  и с явным `drop_reason`.
- Зафиксировать справочник `drop_reason`: `over_budget`, `low_priority`,
  `duplicate`, `superseded`, `expired`, `unverified`, `offloaded`,
  `privacy_restricted`, `invalid_tool_state` и `policy_denied`. Статус
  scratchpad (`draft`, `confirmed`, `recovered`) не является drop reason:
  `recovered` при сборке получает `drop_reason=unverified` или
  `drop_reason=over_budget` и пониженный priority.
- Зафиксировать покрытие `context_ledger_hash`: он считается по ids выбранных
  item, их порядку в собранном контексте, версиям profile, tokenizer и
  нормализатора, обязательному набору, списку отброшенных item с причинами,
  применённым compression/pruning-решениям, fallback-флагу и версии стратегии.
  Одинаковый hash обязан означать одинаковый фактический вход модели; изменение
  порядка, drop/fallback-решения или сжатия меняет hash.
- Зафиксировать, что значит «bounded». Любой bounded вывод обязан иметь
  объявленный в своём этапе числовой лимит и детерминированное правило
  усечения с пометкой факта усечения. Базовые значения, если этап не задаёт
  свои: список ids — не более 100 элементов, текстовая причина — не более 200
  символов, bounded summary — не более 512 токенов, diagnostic-объект — не
  более 4 КБ. Токенные лимиты контекста задаются только через профиль
  (`target/soft/hard` и резервы) и не выражаются словом «bounded».
- Поведение при недоступном estimator: Core не угадывает размер. Если
  tokenizer/estimator нужной версии недоступен, используется консервативный
  fallback-estimator с заведомо завышенной оценкой и пониженным
  `hard_limit_tokens`; факт фиксируется в ledger. Если недоступен и он,
  сборка завершается `BudgetUnavailable` со `stage=estimator_unavailable`.
  Оценка кэшируется по `content_hash` + `tokenizer_version`, поэтому повторная
  валидация на шагах лестницы не пересчитывает неизменные item.
- Зафиксировать модель конкурентности. Сборка контекста выполняется в рамках
  одной задачи и не разделяет изменяемое состояние с другими задачами: budget,
  profile snapshot и выбранный набор — per-call значения. Общими являются
  SQLite-хранилище ledger и artifact store из 01.2. Запись ledger для одного
  model call атомарна: либо появляется полная запись с hash, либо не появляется
  ничего. Параллельные задачи пишут независимые записи и не блокируют друг
  друга; конкуренция разрешается на уровне транзакции SQLite, а не логикой
  planner. `context_ledger_hash` считается по уже зафиксированному составу и не
  зависит от порядка коммитов соседних задач.
- Смена модели или профиля в течение сессии не переписывает прошлое: профиль
  фиксируется на момент конкретного model call и хранится в его записи ledger.
  Следующий вызов собирается заново под новый профиль, ранее отправленный
  контекст не пересчитывается и не «расширяется» задним числом. Если новый
  профиль не вмещает обязательный минимум, применяется обычный
  `BudgetUnavailable`.
- Записи ledger immutable: при апгрейде Core старые записи читаются по своей
  `schema_version` без перезаписи и без пересчёта hash. Миграция может добавлять
  новые поля со значением по умолчанию, но не менять содержимое существующих
  записей.
- Не сохранять в diagnostics сырой prompt, тело памяти или raw tool output;
  сохранять только ids, counts, hashes, policy labels, bounded reasons,
  `compression_ratio`, `offloaded_bytes`, `budget_utilization` по категориям,
  `drop_reason` histogram, `recovery_items_isolated`, latency selection /
  compression / offload и budget counters.

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
- лестница сокращения завершается: property-тест на случайных наборах item
  проверяет, что число итераций не превышает длину лестницы и что каждый
  применённый уровень строго уменьшает `selected_optional_tokens`;
- недоступный estimator даёт консервативный fallback или
  `BudgetUnavailable`, но не молчаливую оценку по умолчанию;
- параллельные задачи не портят ledger: конкурентные записи атомарны, а
  `context_ledger_hash` не зависит от порядка коммитов;
- смена профиля в середине сессии не меняет уже записанные ledger entries;
- pinned item не вытесняет минимально обязательный контекст и не приводит к
  превышению `hard_limit`;
- security tests: diagnostics не содержат prompt, memory body, secret или raw
  tool result.

## Критерии готовности

- каждый model call имеет bounded budget и объяснимый ledger selection;
- ledger и метрики показывают dropped percentage и budget violations;
- `context_ledger_hash` доступен другим планам через versioned Core event/API.
