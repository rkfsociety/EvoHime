# Этап 02.1: Provider contract

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: существующая provider health model и bounded model-gateway
интерфейс. Context Budget Manager здесь не требуется: контракт провайдера не
меняет бюджет.

`budget_id` в snapshot — это ссылка, а не бюджет: Core на этапе создания
snapshot проверяет, что `budget_id` существует и активен (`is_active == true`);
при неуспехе run завершается `failed` с причиной `budget_unavailable` и
selection не запускается. Само распределение бюджета, его убывание и лимиты
остаются контрактом Context Budget Manager и в 02.1 не реализуются. Внутри
`select_route` кандидаты, чья заявленная cost превышает remaining budget,
исключаются тем же фильтром, что и privacy/capability incompatibility — это
может привести к `route_exhausted`, если исключены все кандидаты.
`budget_id` входит в каноническую сериализацию snapshot и в `policy_hashes`
для воспроизводимости.

Разблокирует: 02.2 (локальный провайдер объявляет capabilities) и 02.3
(route selection читает контракт и health overlay).

## Что этап отдаёт наружу

Capability metadata провайдеров, разделение route selection/execution,
неизменяемый policy snapshot запуска, динамический health overlay и
детерминированный circuit breaker/retry contract.

## Термины и границы

- `run` — один Core-owned жизненный цикл пользовательского запроса: от
  классификации и создания snapshot до terminal result (`success`, `failed`,
  `cancelled` или `route_exhausted`). Все попытки fallback принадлежат тому же
  run и имеют один `run_id`. Контракт terminal result:
  - `success` — route ответил в соответствии с retry policy;
  - `failed` — критическая ошибка самого Core до или вне retry-loop (невалидный
    request, policy violation на этапе validate, `budget_unavailable`);
    retry не выполняется, причина пишется в trace как `failure_category` из
    `[invalid_request, policy_violation, budget_unavailable]`;
  - `cancelled` — внешний сигнал отмены (timeout вызывающей стороны,
    cancellation token); текущая попытка прерывается, retry не выполняется;
  - `route_exhausted` — все candidates исключены или исчерпан retry
    (`max_attempts`/`max_elapsed_ms`); в trace пишется `exhaustion_reason` из
    `[no_candidates, max_attempts_reached, max_elapsed_reached,
    all_routes_excluded]`. Наружу (Renderer/Caller) отдаётся только safe-текст
    без внутренних деталей провайдера, точных счётчиков ошибок, auth-данных и
    таймингов, пригодных для reconnaissance — например «No suitable providers
    available» или «All providers returned errors».
- `RoutePolicySnapshot` — зафиксированный в начале run набор кандидатов,
  policy, preference и начального health. Он не изменяется до завершения run.
- `RunHealthOverlay` — отдельное изменяемое состояние только этого run:
  circuit state, failure counters, cooldown и исключённые routes. Overlay не
  добавляет candidates и не расширяет capabilities/policy snapshot.
- `capability_epoch` — монотонная версия metadata конкретного провайдера;
  изменение capability увеличивает epoch и требует нового snapshot.
- `privacy boundary` — максимальный класс данных, разрешённый Core policy для
  route; capability провайдера этот класс не расширяет.
- `policy_hashes` — SHA-256 хэши канонических policy-секций (privacy,
  approval, tools, sandbox, retry), без secrets и prompt.
- `user preference` — подсказка порядка внутри уже разрешённого множества, не
  право выбрать несовместимый или запрещённый route.
- `budget_id` — идентификатор уже созданного budget snapshot; сам бюджет
  остаётся контрактом Context Budget Manager.
- `round-trip hash` — SHA-256 канонического представления snapshot после
  serialize → deserialize; hash до и после должен совпадать.

Renderer получает только read-only/redacted projection и не является источником
ни capability, ни health, ни policy. В Core Rust API snapshot передаётся в
`select_route` и каждую `execute_attempt` как `&RoutePolicySnapshot` (const
reference); `Arc<RoutePolicySnapshot>` допускается для параллельных read-only
операций. Ни одна попытка не получает mutable reference. Renderer не может
изменить snapshot или capability через IPC.

## Что уже есть в коде

Есть: `RouteCandidate` с capabilities, privacy class, cost, latency и флагом
`available`; `select_route` отделён от исполнения; bounded валидация кандидатов
и запроса. В `crates/model-gateway/src/provider_contract.rs` реализованы типы и
машина состояний этого этапа: `CapabilityMetadata` с `schema_version`/
`capability_epoch`/`execution_class`, `RoutePolicySnapshot` с bounded валидацией
и `round_trip_hash`, `PolicyHashes` над каноническим JSON, `RunHealthOverlay` с
circuit breaker, категориями отказов, cooldown и generation-счётчиком,
`RetryConfig` с детерминированным backoff (jitter выводится из `run_id`/
`route_id`, а не из случайности) и `RunTrace`/`AttemptTrace` без секретов,
prompt и raw output. Модуль покрыт 10 тестами.

Нет: исполнения capability probe (есть только `ProbeConfig`/`ProbeResult` как
типы), Core-owned владения snapshot и overlay внутри agent run, TTL-обновления
health из реальных ответов провайдера и подключения selection/execution к
реальному agent run. Пока модуль никем не вызывается — по правилу каталога это
означает отсутствующее поведение, а не закрытый этап.

## Capability contract и startup probe

Capability metadata содержит `schema_version`, `provider_version`,
`capability_epoch`, `tool_calling`, `structured_output`, `context_limit`,
`streaming`, `vision`, `execution_class` (`local`/`cloud`) и `privacy_boundary`.
Отсутствующее поле означает `unsupported`, а не `true`.

Перед публикацией provider выполняет один authenticated startup probe:

- не более 1 попытки, connect timeout 2 s, total timeout 10 s;
- request не более 4 KiB, response не более 64 KiB, без prompt пользователя и
  без tool side effects;
- проверяются structured output, tool-call schema, заявленный context limit,
  streaming framing и privacy boundary;
- cancellation и timeout дают `unavailable`; probe не ретраится;
- `ready` получают только провайдеры с поддерживаемой major schema и всеми
  capabilities, требуемыми их route contract. Частичный capability набор
  сохраняется как metadata, но route исключается, если отсутствует хотя бы одна
  capability из `required_capabilities` запроса.

Неизвестная major-версия capability schema, malformed response или несоответствие
probe заявленной metadata не попадают в snapshot candidates. Поддерживаемая
minor-версия может содержать additive fields, которые Core игнорирует только
после schema validation.

## Snapshot и health lifecycle

В начале run Core в порядке `classify → validate request → collect ready
providers → create snapshot` один раз создаёт snapshot. Чтение persistent
health и создание snapshot выполняются атомарно: под одним read lock
persistent health store Core собирает health для всех ready providers,
формирует `health_snapshot` и создаёт из него snapshot, после чего lock
освобождается. Если persistent health изменится между `classify` и созданием
snapshot, эти изменения не влияют на текущий snapshot и видны только
следующему run. В него попадают только прошедшие schema/probe candidates, их
capability epoch, initial health, `policy_hashes`, preference и `budget_id`.
Snapshot замораживается до terminal result и уничтожается после записи trace.

Health snapshot содержит `status`, `observed_at`, `ttl`, `circuit_state` и
`last_failure_category`. TTL проверяется при выборе route; просроченное health
наблюдение даёт `stale`, а не автоматически `ready`.

`RunHealthOverlay` создаётся из health snapshot один раз при старте run, до
первого вызова `select_route`: для каждого candidate из snapshot Core
инициализирует `route_status` из соответствующего `snapshot.health` (`open`,
если health уже `stale`/`open`, иначе `closed`), обнуляет
`attempts_per_route` и `failure_count_by_category`, и заполняет `generation`
стартовым значением. Инициализация локальна для run и отдельного lock на
persistent health не требует.

Каждое обновление overlay выполняется через Core-owned mutex/`RwLock` под
write lock (или эквивалентный CAS в реализации), с проверкой `run_id`,
monotonic `attempt_id` и generation: перед записью overlay проверяет, что
переданный `attempt_id`/`generation` не устарели относительно текущего
значения, атомарно инкрементирует `generation` и коммитит изменение счётчиков
и `route_status` одной операцией. Старая generation не может затереть новую;
после commit selection читает overlay под read lock. Overlay никогда не
публикуется renderer как источник истины.

Таким образом, snapshot фиксирует начальное состояние, а overlay отражает
динамические отказы текущего run. Изменения persistent health, capabilities и
policy влияют только на следующий run.

## Selection, retry и circuit breaker

Перед каждой попыткой Core вызывает `select_route(&snapshot, &overlay,
request, attempt_index)`. Алгоритм:

1. отбрасывает candidates с несовместимой schema, отсутствующей required
   capability, privacy/approval/tool/sandbox violation, stale/`open` circuit,
   исчерпанным route или превышенным per-route limit;
2. применяет фиксированный порядок policy из обзора плана: privacy → offline →
   approval/tool → context/capability → health/circuit → evaluation → budget →
   user preference → lexical `route_id`;
3. среди оставшихся candidates выбирает первый по детерминированному
   tie-break: `health.status` (`ready` > `cooldown`; `open` уже отброшен
   фильтром на шаге 1, `half_open` в 02.1 не производится) → latency по
   возрастанию → cost по возрастанию → порядок `user preference` из
   запроса → lexical `route_id` как финальный break. Score здесь — не
   числовое значение, а порядок candidates после фильтров и tie-break;
   «первый route» значит первый в этом порядке. Reason записывается как
   комбинация применённого фильтра/критерия tie-break;
4. передаёт тот же snapshot по `&` в execution. Execution не выбирает другой
   route сам.

Ошибки `timeout`, `connection_refused`, `5xx` и `malformed_response` открывают
circuit после порога категории. `429` увеличивает отдельный counter и после
порога переводит route в `cooldown`; policy/approval denial, invalid request и
cancellation circuit не открывают. При открытии overlay атомарно помечает
route `open`, увеличивает generation и пишет `circuit_opened_during_run`.
Текущая retry-loop немедленно прекращается; snapshot при этом остаётся тем же.

Lifecycle circuit breaker в пределах run:
- `circuit_state` открывается в overlay при превышении
  `health.failure_threshold`/`health.rate_limit_threshold`; на всё оставшееся
  время run открытый circuit исключает route из selection и обратно не
  закрывается — «половинного открытия» (`half-open` retry) внутри run в 02.1
  нет;
- по завершении run `circuit_state` из overlay в persistent health
  автоматически не копируется; persistent health обновляется отдельно, на
  основе накопленных `failure_category`/`failure_count`, и это остаётся вне
  scope 02.1 (owner — provider health model);
- `cooldown_ms` относится к persistent health и следующему run: route,
  помеченный `cooldown`, исключается из selection нового run, пока
  `observed_at + cooldown_ms < current_time`; в текущем run cooldown route
  не «размораживается»;
- сброс/half-open probe для переподтверждения capabilities закрытого route не
  реализуется в 02.1; переподтверждение выполняется startup probe следующего
  run.

Следующая попытка выбирается повторным вызовом `select_route` с тем же
snapshot и обновлённым overlay. Порядок candidates не перестраивается по
внешнему health; исключённый route не возвращается до конца run. Если после
исключения route остаются, применяется retry policy и выполняется следующий
route. Если candidates не осталось или достигнут run limit, Core возвращает
`route_exhausted` с безопасной причиной, не маскирует его под success и не
делает unbounded retry.

Параметры Core-конфигурации (с bounded minimum/maximum validation):

```text
retry.max_attempts                 = 3
retry.max_attempts_per_route       = 2
retry.initial_backoff_ms           = 250
retry.max_backoff_ms               = 4000
retry.jitter_ratio                 = 0.20
retry.max_elapsed_ms               = 15000
health.failure_threshold           = 2
health.rate_limit_threshold        = 3
health.cooldown_ms                 = 30000
```

Backoff — exponential с cap и deterministic jitter от `hash(run_id | route_id |
attempt_id)`, поэтому replay воспроизводим. Конфигурация копируется в
snapshot/policy hash и не меняется внутри run.

## Сериализация, trace и обратная совместимость

Каноническая сериализация snapshot содержит `schema_version`, `policy_version`,
`run_id`, candidates/epochs, initial health, policy hashes, preference и
budget id. Prompt, secrets, raw provider output и credentials запрещены.

Deserializer отклоняет неизвестные поля, дубликаты, missing required fields,
unsupported major schema и round-trip hash mismatch. Для предыдущей
поддерживаемой major версии существует явный мигратор `vN → vN+1`, который
заполняет только безопасные defaults, заново валидирует capabilities и policy,
а затем пересчитывает hash; downgrade и silent best-effort parsing запрещены.
Неподдерживаемая версия даёт `snapshot_incompatible` и не запускает provider.

Trace — Core-owned redacted JSONL/event record на каждый run. Он содержит
`run_id`, snapshot/policy hash, schema versions, ordered attempts,
`route_id`, capability epoch, selection reason, failure category, backoff,
overlay generation и boolean `circuit_opened_during_run`. Prompt, secrets и
raw output не пишутся. Счётчики `provider_attempts_total`,
`provider_failures_total{category}`, `circuit_open_total`,
`route_exhausted_total` и gauge открытых circuits публикуются в локальную
diagnostics telemetry с bounded cardinality.

## Проверки

- contract tests для полной и частичной capability metadata;
- probe tests на timeout, size limits, malformed output и schema major/minor;
- snapshot tests: `&`-передача, renderer не может изменить поля, stable order;
- concurrent overlay tests на lost update, stale generation и monotonic attempt;
- health TTL, thresholds, cooldown и категории circuit breaker;
- deterministic next-route/retry tests с backoff, jitter, max attempts и
  `route_exhausted`;
- snapshot serialize/deserialize, unknown fields, migration и round-trip hash;
- trace/telemetry tests без secrets и raw provider output.

## Критерии готовности

- решение route и порядок попыток воспроизводимы по immutable snapshot и
  deterministic retry policy;
- capability провайдера нельзя переопределить из renderer, а частичный набор
  не допускает запрос без всех required capabilities;
- snapshot policy, overlay ownership и категории circuit breaker определены и
  протестированы;
- circuit breaker атомарно взаимодействует с immutable snapshot внутри run и
  исключает открытый route из дальнейшего выбора;
- startup probe имеет конкретные bounded limits;
- schema migration, round-trip hash, trace и observability contract покрыты.
