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

### Бюджет и обязательный минимум

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
- Внутри `minimum_viable_context` действует фиксированный порядок частей:
  safety/system policy → approval/permission semantics → текущий user prompt →
  состояние незавершённого tool-call → cancellation context. Порядок не даёт
  права сокращать младшие части; он определяет только детерминированный выбор
  значения `missing_part` (первая по этому порядку часть, на которой сумма
  превысила лимит) и порядок частей в собранном контексте.
- Профиль обязан содержать `absolute_mvc_max_limit` — верхнюю границу размера
  обязательного минимума. Значение по умолчанию: `floor(0.40 *
  max_context_tokens)`. Если `mandatory_tokens > absolute_mvc_max_limit`, Core
  возвращает `BudgetUnavailable` со `stage=mandatory_overflow` и
  `missing_part`, не пытаясь ни сокращать MVC, ни занимать резервы. Эта
  проверка выполняется раньше проверки против `hard_limit_tokens`, поэтому
  «раздувшийся» системный промпт даёт понятный отказ, а не бесконечное
  сокращение необязательной части.

### Отказ сборки: `BudgetUnavailable`

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
- Сценарии `stage`:
  - `mandatory_overflow` — обязательный минимум превышает
    `absolute_mvc_max_limit` либо `mandatory_tokens + reserves_total >
    hard_limit_tokens` до начала selection;
  - `drops_exhausted` — лестница сокращения пройдена целиком, финальная
    проверка всё ещё не выполняется;
  - `estimator_unavailable` — недоступны и основной, и fallback estimator;
  - `provider_replan_failed` — провайдер вернул context-length error, был
    выполнен ровно один deterministic re-plan с профилем `v+1`, и этот re-plan
    либо снова получил context-length error, либо сам завершился отказом
    сборки (обязательный минимум не помещается в уменьшенный
    `hard_limit_tokens`). В обоих случаях внешний `stage` —
    `provider_replan_failed`, а внутренняя причина второго отказа попадает в
    bounded `missing_part`.

### Профиль модели и бюджетная арифметика

- Ввести versioned `ModelContextProfile`, выбираемый по provider/model. Профиль
  обязан содержать `max_context_tokens`, `target_tokens`, `soft_limit_tokens`,
  `hard_limit_tokens`, `absolute_mvc_max_limit`, `tool_schema_reserve`,
  `tool_call_reserve`, `final_answer_reserve`, `streaming_reserve`,
  `retry_reserve`, `low_priority_cutoff` и `offload_threshold_bytes`. Базовый
  fallback-профиль: `target_tokens=60%`, `soft_limit_tokens=75%`,
  `hard_limit_tokens=85%` от
  заявленного окна модели, минимум 1024 токена под tool-call и 2048 под
  final answer; значения проверяются на совместимость с обязательным минимумом,
  а сумма обязательного минимума и всех резервов не может превышать
  `hard_limit_tokens`. Неизвестная модель использует fallback-профиль и не
  может обойти эти ограничения.
- Зафиксировать бюджетную арифметику явными формулами. Обозначения:
  `reserves_total = tool_schema_reserve + tool_call_reserve +
  final_answer_reserve + streaming_reserve + retry_reserve`;
  `context_tokens = mandatory_tokens + selected_optional_tokens`.
  - Валидность профиля (проверяется при загрузке, невалидный профиль не
    используется): `0 < target_tokens < soft_limit_tokens < hard_limit_tokens
    <= max_context_tokens`, `target_tokens + reserves_total <=
    soft_limit_tokens`, `absolute_mvc_max_limit + reserves_total <=
    hard_limit_tokens`.
  - Целевое состояние: `context_tokens + reserves_total <= target_tokens +
    reserves_total <= soft_limit_tokens`.
  - Жёсткий инвариант: `context_tokens + reserves_total <= hard_limit_tokens`.
  - Верхняя граница необязательной части: `selected_optional_tokens <=
    hard_limit_tokens - reserves_total - mandatory_tokens`.
  Таким образом `target_tokens` — граница для самого контекста, резервы
  считаются сверх него, а совместимость обеих величин гарантируется правилами
  валидности профиля, а не соглашением реализации.
- `target_tokens` расходуется на контекст, а резервы считаются отдельно и не могут
  быть заняты history или schemas. Профиль и фактическое распределение
  сохраняются в ledger; provider usage после ответа обновляет диагностику.
- Поведение по порогам:
  - `context_tokens + reserves_total <= target_tokens + reserves_total` —
    сборка завершена, лестница не запускается;
  - превышение `target_tokens` при `context_tokens + reserves_total <=
    soft_limit_tokens` — допустимо, фиксируется в ledger как
    `budget_utilization`, сокращение не запускается;
  - превышение `soft_limit_tokens` — Core запускает лестницу сокращения и идёт
    по её уровням, пока `context_tokens + reserves_total` не окажется не выше
    `target_tokens + reserves_total` либо пока лестница не будет исчерпана;
  - превышение `hard_limit_tokens` после исчерпания лестницы —
    `BudgetUnavailable` со `stage=drops_exhausted`.
  То есть `soft_limit_tokens` — порог запуска сокращения, `target_tokens` — его
  цель, `hard_limit_tokens` — граница отказа.
- `retry_reserve` резервирует место под повтор запроса при transport-ошибке
  провайдера (обрыв соединения, 5xx до начала генерации), когда provider policy
  такие повторы допускает; на context-length error и на `BudgetUnavailable` он
  не распространяется — там повтор запрещён. Если повторы отключены,
  `retry_reserve = 0`. При лестнице сокращения он освобождается первым среди
  необязательных резервов.
- Профили хранятся в versioned каталоге, поставляемом со сборкой Core
  (декларативный файл конфигурации), и могут перекрываться пользовательским
  конфигом того же формата. Любое изменение значений — новый `profile_version`;
  правка «на месте» без смены версии запрещена, потому что версия входит в
  `context_ledger_hash`. Загруженный профиль валидируется по правилам выше;
  невалидный профиль отклоняется с diagnostic, и используется fallback-профиль.

### Лестница сокращения

- Финальная проверка выполняется после selection, compression и fallback:
  `mandatory_tokens + selected_optional_tokens + reserves <= hard_limit_tokens`.
  Если условие не выполняется, Core переходит к следующему уровню лестницы
  сокращения, а не повторяет тот же уровень. Каждый уровень применяется
  не более одного раза за сборку и обязан строго уменьшать
  `selected_optional_tokens`; уровень, не давший уменьшения, считается
  исчерпанным немедленно. Число итераций ограничено длиной лестницы, поэтому
  цикл завершается всегда. После последнего уровня Core возвращает
  `BudgetUnavailable` со `stage=drops_exhausted` без model call.
- Лестница конечна, упорядочена и задана заранее. Полный список уровней,
  условий активации и проставляемых причин:

  | № | Уровень | Условие для item | `drop_reason` |
  |---|---------|------------------|---------------|
  | L1 | expired / duplicate / superseded | `ttl` истёк (`now > created_at + ttl`), либо истёк `retention`; либо совпадение `content_hash` с уже выбранным item; либо новая ревизия того же `parent_id`/ключа | `expired`, `duplicate`, `superseded` |
  | L2 | low-priority optional | `effective_priority < profile.low_priority_cutoff` (по умолчанию 30), item не входит в MVC | `low_priority` |
  | L3 | самые старые tool outputs | `kind=tool_result`, пара tool-call/result завершена, сортировка по `created_at` возрастанию; незавершённые пары не трогаются | `stale_tool_output` |
  | L4 | offload крупных item в artifact store | `bytes > profile.offload_threshold_bytes` (по умолчанию 32 КБ), artifact store доступен, item допускает offload по privacy | `offloaded` |
  | L5 | сжатие истории | доступен summarizer из 01.3, есть хотя бы два item `kind=history` или `kind=tool_result` вне MVC | причина не проставляется исходным item: они заменяются summary со связью `summary_id -> source_ids` |
  | L6 | отказ от необязательных резервов | фиксированный порядок: `retry_reserve` → `streaming_reserve` → `tool_schema_reserve` сверх фактического размера схем; `tool_call_reserve` и `final_answer_reserve` не сокращаются никогда | причина не проставляется item |

  Внутри уровня порядок отбрасывания детерминированный: сначала по возрастанию
  `effective_priority`, затем по возрастанию `created_at`, затем по
  `content_hash`, затем по `id` лексикографически. Уровень применяется целиком:
  он отбрасывает ровно столько item по этому порядку, сколько нужно для
  выполнения финальной проверки, и останавливается раньше, если проверка уже
  выполнена.
- `pinned` — не уровень лестницы, а модификатор порядка: pinned item внутри
  каждого уровня стоит последним и отбрасывается только тогда, когда остальные
  кандидаты уровня исчерпаны. Отдельного «уровня отказа от pinned» нет.
- Уровни L4 и L5 зависят от возможностей, поставляемых этапами 01.2 и 01.3.
  Core определяет их доступность через capability-пробу Core-компонента при
  старте сборки, а не по факту ошибки в середине уровня. Недоступный artifact
  store или summarizer означает, что уровень немедленно считается исчерпанным,
  а в ledger пишется diagnostic (`artifact_store_unavailable`,
  `summarizer_unavailable`); лестница продолжается со следующего уровня. Отказ
  внутри уже начатого уровня (ошибка записи артефакта, invalid summary)
  трактуется так же: изменения уровня откатываются, уровень помечается
  исчерпанным, исходные item остаются выбранными. Поэтому 01.1 реализуется и
  тестируется до 01.2/01.3: без них лестница состоит из L1–L3 и L6.

### Версионирование и миграции

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
- Механизм миграции ledger: схема таблиц хранит собственную `schema_version`,
  миграции только additive (`ALTER TABLE ADD COLUMN` с SQL-значением по
  умолчанию либо `NULL`). Значение по умолчанию задаётся одновременно в SQL и в
  reader-коде, чтобы записи, созданные до миграции, читались одинаково при
  любом порядке применения. Записи со старой `schema_version` читаются своим
  reader'ом и не переписываются; hash не пересчитывается. Несовместимое
  major-изменение выполняется созданием новой таблицы рядом со старой:
  старые записи остаются доступны только на чтение, новые пишутся в новую
  таблицу. Каждая миграция сопровождается тестом на «золотых» записях
  предыдущей версии.

### Оценка токенов

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
  отказ завершает вызов через `BudgetUnavailable` со
  `stage=provider_replan_failed`; каскад re-plan запрещён.
- Поведение при недоступном estimator: Core не угадывает размер. Если
  tokenizer/estimator нужной версии недоступен, используется консервативный
  fallback-estimator; факт фиксируется в ledger флагом `fallback_estimator`.
  Если недоступен и он, сборка завершается `BudgetUnavailable` со
  `stage=estimator_unavailable`.
- Спецификация fallback-estimator (versioned, как и основной):
  - оценка item: `estimated_tokens = ceil(utf8_bytes / 2) + 16`, плюс 8 токенов
    на каждое сообщение chat-template и `ceil(utf8_bytes(schema) / 2)` на
    tool-schema; округление всегда вверх;
  - пороги профиля масштабируются: `hard_limit_tokens_fallback = floor(0.70 *
    hard_limit_tokens)`, `soft_limit_tokens_fallback = floor(0.70 *
    soft_limit_tokens)`, `target_tokens_fallback = floor(0.70 *
    target_tokens)`; резервы не уменьшаются;
  - допустимый over-estimate fallback-оценки — до 100% от фактических prompt
    tokens; систематическое превышение этой границы на 100 подряд вызовах
    фиксируется как diagnostic и повод уточнить коэффициенты, но занижение
    остаётся дефектом и в fallback-режиме;
  - если после масштабирования порогов `mandatory_tokens + reserves_total >
    hard_limit_tokens_fallback`, сборка завершается `BudgetUnavailable` со
    `stage=mandatory_overflow`, а не отправляет вызов на грани окна.
- Оценка кэшируется по ключу `content_hash` + `tokenizer_version` +
  `normalizer_version` + версия chat-template из профиля, поэтому повторная
  валидация на шагах лестницы не пересчитывает неизменные item и смена любого
  из версионируемых компонентов не даёт стухший кэш-хит.

### `ContextItem` и хеширование

- Ввести `ContextItem` с `id`, `task_id`, `session_id`, `parent_id`, `kind`,
  `source`, `priority`, `trust`, `privacy`, `created_at`, `last_used_at`,
  `ttl`, `retention`, `pinned`, `version`, `tokenizer_version`, `content_hash`,
  `bytes`, `estimated_tokens`, `selected` и `drop_reason`.
- `priority` — целое 0..100, больше значит важнее. `effective_priority`
  вычисляется детерминированно: базовый `priority`; `pinned=true` даёт
  `max(priority, 90)`; scratchpad-статус `recovered` даёт `min(priority, 20)`.
  Правила применяются в этом порядке, поэтому pinned recovered-запись получает
  90 и всё равно остаётся необязательной.
- `content_hash` считается по нормализованному содержимому item и служит
  единым основанием для дедупликации, `drop_reason=duplicate`, conflict
  detection и дедупликации artifact store. Спецификация:
  - алгоритм — SHA-256, представление — строчный hex;
  - hash-вход — конкатенация `normalizer_version`, байта-разделителя `0x00`,
    `kind`, `0x00` и нормализованного содержимого; версия нормализатора входит
    в hash input, а не только в кэш-ключ;
  - нормализация текста в фиксированном порядке: декодирование в UTF-8 →
    Unicode NFC → перевод всех `\r\n` и `\r` в `\n` → удаление завершающих
    пробелов в каждой строке → удаление завершающих пустых строк; ведущие
    пробелы сохраняются;
  - нормализация JSON — каноническая форма: объекты сортируются по ключам по
    возрастанию кодовых точек UTF-8, незначащие пробелы удаляются, строки
    нормализуются по правилу выше, числа выводятся в фиксированном
    представлении (целые — без экспоненты и без завершающего `.0`; дробные — в
    кратчайшем round-trip представлении), порядок элементов массива значим и не
    меняется;
  - двоичное содержимое хешируется как есть, без нормализации, с префиксом
    `kind`;
  - реализация сопровождается эталонными векторами (текст с CRLF, текст с
    комбинирующими символами, JSON с переставленными ключами, JSON с числами
    `1`, `1.0` и `1e0`, пустая строка), зафиксированными в тестах.
- `pinned` выставляется только пользовательской командой, поднимает эффективный
  priority и выводит item из обычного pruning. Pin не может нарушить
  `hard_limit`, вытеснить минимально обязательный контекст или удержать item с
  истёкшим retention: при нехватке бюджета pinned item отбрасывается последним
  и с явным `drop_reason`.
- Зафиксировать справочник `drop_reason`: `over_budget`, `low_priority`,
  `duplicate`, `superseded`, `expired`, `unverified`, `offloaded`,
  `stale_tool_output`, `privacy_restricted`, `invalid_tool_state` и
  `policy_denied`. Статус
  scratchpad (`draft`, `confirmed`, `recovered`) не является drop reason:
  `recovered` при сборке получает `drop_reason=unverified` или
  `drop_reason=over_budget` и `effective_priority` по правилу выше.

### `context_ledger` и его hash

- Зафиксировать покрытие `context_ledger_hash`: он считается по ids выбранных
  item, их порядку в собранном контексте, версиям profile, tokenizer и
  нормализатора, обязательному набору, списку отброшенных item с причинами,
  применённым compression/pruning-решениям, fallback-флагу и версии стратегии.
  Одинаковый hash обязан означать одинаковый фактический вход модели; изменение
  порядка, drop/fallback-решения или сжатия меняет hash.
- Момент вычисления однозначен: hash считается один раз после selection,
  compression и успешной финальной проверки — то есть когда состав и порядок
  контекста уже зафиксированы — и до отправки model call. Он не является входом
  ни для selection, ни для лестницы, поэтому цикла зависимостей нет и второй
  проход не нужен. Потребители 02.5, 03.1, 04.3 и 05.3 получают уже готовый
  hash из Core event, который публикуется до model call; для отказа сборки
  публикуется hash частичной сборки с флагом отказа. Валидация upstream всегда
  сравнивает hash с уже записанным ledger entry, а не пересчитывает контекст.
- Структура записи `context_ledger` (одна запись на один model call):
  `id` TEXT PK, `schema_version` INTEGER, `task_id` TEXT, `session_id` TEXT,
  `model_call_id` TEXT, `created_at` INTEGER (unix ms), `provider` TEXT,
  `model` TEXT, `profile_version` TEXT, `profile_snapshot` TEXT (JSON),
  `tokenizer_version` TEXT, `normalizer_version` TEXT, `strategy_version` TEXT,
  `mandatory_tokens` INTEGER, `selected_optional_tokens` INTEGER,
  `reserves_tokens` INTEGER, `estimated_prompt_tokens` INTEGER,
  `selected_items` TEXT (JSON: упорядоченный список `{id, estimated_tokens}`),
  `dropped_items` TEXT (JSON: список `{id, drop_reason}`),
  `ladder_levels_applied` TEXT (JSON), `compression` TEXT (JSON:
  `summary_id -> source_ids`, `compression_ratio`), `fallback_estimator`
  INTEGER (0/1), `replan_of` TEXT NULL, `outcome` TEXT
  (`sent` | `budget_unavailable`), `budget_unavailable` TEXT NULL (JSON: `code`,
  `stage`, `missing_part`, `required_tokens`, `available_tokens`),
  `context_ledger_hash` TEXT.
- Фактический usage провайдера пишется не в эту запись, а в append-only таблицу
  `context_ledger_usage` (`ledger_id`, `actual_prompt_tokens`,
  `actual_completion_tokens`, `estimator_drift`, `recorded_at`). Так запись
  ledger остаётся immutable и hash-стабильной, а диагностика после ответа всё
  равно доступна.
- Записи ledger immutable: при апгрейде Core старые записи читаются по своей
  `schema_version` без перезаписи и без пересчёта hash. Миграция может добавлять
  новые поля со значением по умолчанию, но не менять содержимое существующих
  записей.
- Ротация: записи ledger хранятся, пока выполняется хотя бы одно условие —
  возраст менее 30 дней или запись относится к одной из последних 200 сессий.
  Очистка выполняется фоновой задачей Core, удаляет записи целиком (вместе со
  строками `context_ledger_usage`) и сама попадает в метрики как
  `context_ledger_pruned_total`. Записи, на которые ссылается неэкспортированный
  receipt из 03.4, не удаляются; сами receipts хранят `context_ledger_hash`
  независимо, поэтому очистка ledger не ломает цепочку.

### Конкурентность

- Зафиксировать модель конкурентности. Сборка контекста выполняется в рамках
  одной задачи и не разделяет изменяемое состояние с другими задачами: budget,
  profile snapshot и выбранный набор — per-call значения. Общими являются
  SQLite-хранилище ledger и artifact store из 01.2. Запись ledger для одного
  model call атомарна: либо появляется полная запись с hash, либо не появляется
  ничего. Параллельные задачи пишут независимые записи и не блокируют друг
  друга; конкуренция разрешается на уровне транзакции SQLite, а не логикой
  planner. `context_ledger_hash` считается по уже зафиксированному составу и не
  зависит от порядка коммитов соседних задач.
- Параметры конкурентности: SQLite в режиме WAL, `busy_timeout = 5000` мс,
  запись ledger выполняется одной транзакцией `BEGIN IMMEDIATE`, чтения
  диагностики — в режиме `read committed` из WAL-снимка без блокировки писателей.
  Максимум одновременных сборок контекста ограничен Core-параметром
  `max_concurrent_model_calls` (по умолчанию 4); превышение ставит задачу в
  очередь, а не расширяет параллельность. При `SQLITE_BUSY` после истечения
  timeout запись ledger повторяется до 3 раз с экспоненциальной задержкой
  (50/100/200 мс); исчерпание повторов означает отказ шага с diagnostic
  `ledger_write_failed` и без model call. Повтор записи в БД не является
  запрещённым retry model call: сам вызов модели не повторяется никогда.
- Смена модели или профиля в течение сессии не переписывает прошлое: профиль
  фиксируется на момент конкретного model call и хранится в его записи ledger.
  Следующий вызов собирается заново под новый профиль, ранее отправленный
  контекст не пересчитывается и не «расширяется» задним числом. Если новый
  профиль не вмещает обязательный минимум, применяется обычный
  `BudgetUnavailable`.

### Диагностика, метрики и bounded-вывод

- Зафиксировать, что значит «bounded». Любой bounded вывод обязан иметь
  объявленный в своём этапе числовой лимит и детерминированное правило
  усечения с пометкой факта усечения. Базовые значения, если этап не задаёт
  свои: список ids — не более 100 элементов, текстовая причина — не более 200
  Unicode-символов (кодовых точек после NFC, не байт и не графемных кластеров),
  bounded summary — не более 512 токенов, diagnostic-объект — не более 4 КБ
  (в байтах UTF-8 после сериализации). Токенные лимиты контекста задаются
  только через профиль (`target/soft/hard` и резервы) и не выражаются словом
  «bounded».
- Не сохранять в diagnostics сырой prompt, тело памяти или raw tool output;
  сохранять только ids, counts, hashes, policy labels, bounded reasons,
  `compression_ratio`, `offloaded_bytes`, `budget_utilization` по категориям,
  `drop_reason` histogram, `recovery_items_isolated`, latency selection /
  compression / offload и budget counters.
- Метрики этапа: `context_drops_total{reason}`,
  `context_budget_utilization` (histogram по категориям, читается как p95),
  `context_estimator_drift` (histogram относительной погрешности),
  `context_ladder_level_applied_total{level}`,
  `context_replan_total{outcome}`,
  `context_budget_unavailable_total{stage}`,
  `context_selection_latency_ms` (histogram),
  `context_offloaded_bytes_total`, `context_ledger_pruned_total`.
- Пороги alerting: p95 `context_estimator_drift` выше 5% на окне из 100 вызовов;
  любое зафиксированное занижение оценки; доля вызовов с re-plan выше 1% на том
  же окне; любой `context_budget_unavailable_total{stage=estimator_unavailable}`;
  любой `ledger_write_failed`.
- Требования к производительности: selection и финальная проверка для 1000
  `ContextItem` укладываются в p95 50 мс на целевой машине разработки при
  прогретом кэше оценки; расчёт `content_hash` для неизменного item не
  повторяется в пределах одной сборки; запись ledger — p95 не более 20 мс.
  Числа проверяются бенчмарком и являются регрессионным порогом, а не SLA перед
  пользователем.

## Проверки

- unit-тесты budget arithmetic, model profiles, tokenizer overhead и
  deterministic ordering;
- валидация профиля: значения, нарушающие формулы совместимости
  (`target + reserves > soft`, `absolute_mvc_max_limit + reserves > hard`),
  отклоняются при загрузке, а не в момент сборки;
- property-тесты: budget никогда не превышается, минимально обязательный
  контекст и permissions сохраняются, output остаётся bounded даже при ошибке
  estimator;
- профиль неизвестной модели не может обойти обязательный минимум: несовместимые
  значения дают `BudgetUnavailable`, а не молчаливое превышение;
- `mandatory_tokens > absolute_mvc_max_limit` даёт `stage=mandatory_overflow`
  с детерминированным `missing_part` до запуска selection;
- эталонные векторы `content_hash`: CRLF/LF, NFC/NFD, переставленные ключи
  JSON, `1` / `1.0` / `1e0`, пустая строка — дают ожидаемые фиксированные
  значения; смена `normalizer_version` меняет hash того же содержимого;
- ledger hash меняется при изменении порядка item, версии profile/tokenizer или
  применённого compression-решения и совпадает при идентичном входе модели;
  hash публикуется до model call и совпадает с записанным в ledger;
- context-length error провайдера даёт ровно один re-plan, затем
  `BudgetUnavailable` со `stage=provider_replan_failed`; каскад re-plan не
  возникает;
- лестница сокращения завершается: property-тест на случайных наборах item
  проверяет, что число итераций не превышает длину лестницы и что каждый
  применённый уровень строго уменьшает `selected_optional_tokens`;
- порядок уровней воспроизводим: одинаковый вход даёт одинаковую
  последовательность `ladder_levels_applied` и одинаковые `drop_reason`;
- недоступные artifact store или summarizer пропускают L4/L5 с diagnostic, а
  не роняют сборку; лестница из L1–L3 и L6 работает без этапов 01.2 и 01.3;
- недоступный estimator даёт консервативный fallback с пониженными порогами
  (`0.70`) или `BudgetUnavailable`, но не молчаливую оценку по умолчанию;
  fallback-оценка никогда не ниже фактического usage на фикстурах;
- кэш оценки инвалидируется при смене `tokenizer_version`, `normalizer_version`
  и версии chat-template;
- параллельные задачи не портят ledger: конкурентные записи атомарны, а
  `context_ledger_hash` не зависит от порядка коммитов; `SQLITE_BUSY`
  обрабатывается повтором записи и не приводит к повтору model call;
- миграция ledger: «золотые» записи предыдущей `schema_version` читаются после
  additive-миграции без перезаписи и без пересчёта hash;
- ротация удаляет записи целиком вместе со строками `context_ledger_usage` и не
  удаляет записи, на которые ссылается неэкспортированный receipt;
- смена профиля в середине сессии не меняет уже записанные ledger entries;
- pinned item не вытесняет минимально обязательный контекст и не приводит к
  превышению `hard_limit`;
- security tests: diagnostics не содержат prompt, memory body, secret или raw
  tool result;
- бенчмарк selection на 1000 item и записи ledger удерживает объявленные
  регрессионные пороги.

## Критерии готовности

- каждый model call имеет bounded budget и объяснимый ledger selection;
- формулы соотношения `target_tokens`, `soft_limit_tokens`, `hard_limit_tokens`
  и резервов документированы, проверяются при загрузке профиля и покрыты
  property-тестами;
- полный упорядоченный список уровней лестницы с условиями активации и
  `drop_reason` реализован; property-тест подтверждает завершаемость и строгое
  уменьшение `selected_optional_tokens`;
- `content_hash` специфицирован (SHA-256, порядок нормализации, канонический
  JSON) и зафиксирован эталонными векторами;
- ledger и метрики показывают dropped percentage, budget violations,
  `estimator_drift` и долю re-plan; пороги alerting заданы;
- структура записи `context_ledger`, механизм миграции и стратегия ротации
  документированы и покрыты тестами;
- отказы сборки дают документированный `stage`: `mandatory_overflow`,
  `drops_exhausted`, `estimator_unavailable` или `provider_replan_failed`;
- контракт взаимодействия с artifact store (01.2) и summarizer (01.3)
  описан как capability-проба: их отсутствие не блокирует этап;
- `context_ledger_hash` доступен другим планам через versioned Core event/API
  до model call.
