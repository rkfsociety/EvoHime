# Этап 02.2: Local provider

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 02.1 — локальный провайдер объявляет capabilities по общему
контракту (`CapabilityMetadata`, `capability_epoch`, `HealthStatus`) и не
вводит своего параллельного словаря. Context Budget Manager здесь не требуется:
локальный адаптер не касается бюджета.

Опциональные: 02.3 routing. До его появления адаптер вызывается только
контрактными тестами и диагностикой: `ToolAgent` по-прежнему ходит в
`chat_with_tools_for_route("default")`, а `RoutingMode::LocalFirst` продолжает
работать так, как работает сегодня. Cloud fallback этот этап не добавляет и не
расширяет.

Разблокирует: 02.3 — без локального route падать некуда.

Не является зависимостью, но входит в объём этапа: канал команд от Core к
supervisor. Сегодня его нет (см. «Что уже есть в коде»), поэтому 02.2 обязан
его создать; ссылаться на него как на «уже существующую границу проекта»
нельзя. Выбор конкретной SLM и launcher остаётся за ADR из 02.0; этот этап
фиксирует контракт адаптера, а не бренд модели.

## Что этап отдаёт наружу

Loopback-only local route с честным статусом доступности и supervisor-owned
жизненным циклом процесса модели.

## Что уже есть в коде

Локального провайдера нет. В `crates/model-gateway/src/providers/` есть только
`literouter`, `openai_compatible` и `mock`.

Важно для планирования — четыре факта о существующем коде, каждый из которых
меняет объём этапа:

- `local_route_unavailable` в `routing_runtime.rs:358` **не является отказом**.
  Это `fallback.reason` режима `RoutingMode::LocalFirst`: когда ни один local
  candidate не доступен, фильтр снимается и `select_route` выбирает из всех
  кандидатов, включая cloud (`lib.rs:551` и тест в `routing_runtime.rs:468`
  ожидают ровно `selected_route == "cloud"`). То есть падать сегодня есть куда,
  и падение это молчаливое с точки зрения privacy: cloud выбирается без
  проверки sensitivity. 02.2 обязан не сломать этот код и не выдавать его за
  bounded refusal; ограничить его правом policy — задача 02.3;
- принадлежность к local определяется подстрокой: `is_local_route` считает
  локальным любой candidate, у которого `route_id` или `model` содержит
  `local`/`offline`. Регистрация route с таким id немедленно меняет поведение
  `LocalFirst`/`Offline` без единой строки в routing. Этап обязан либо
  зарегистрировать route с явным `execution_class = local` из 02.1 и заменить
  подстрочную эвристику на него, либо честно записать, что эвристика осталась;
- `ProviderKind` — закрытый enum (`LiteRouter`, `OpenAICompatible`, `Mock`) с
  `parse`/`as_str`, а `ModelProvider` умеет только `stream_chat`,
  `stream_with_thinking` и `chat_with_tools`. В trait нет ни capability probe,
  ни health, ни явной отмены (отмена сегодня — это drop stream). Поэтому
  «подключить через существующий provider trait» означает расширить и enum, и
  trait: добавить вариант `Local`, метод получения `CapabilityMetadata` и
  метод отмены, принимающий cancellation token;
- supervisor (`crates/evohime-supervisor/src/windows_supervisor.rs`) сегодня
  управляет **одним** дочерним процессом — самим Core: `JobObject::create`
  ставит только `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (ни memory, ни CPU
  limits), `assign` привязывает один child, а команд извне supervisor не
  принимает вовсе. Named pipe принадлежит Core (`evohime-core/src/
  pipe_server.rs`): Core — сервер для shell, а supervisor общается с ним
  односторонне, через protected launch context. Канала «Core → supervisor» не
  существует, как и второго Job Object, keyed-by-request lifecycle и
  multi-child учёта.

## Содержание

- Добавить OpenAI-compatible local endpoint adapter с loopback-only policy,
  вариант `ProviderKind::Local` и реализацию расширенного `ModelProvider`.
- Добавить authenticated канал команд `Core → supervisor` (Windows named pipe
  с owner-only DACL и той же схемой подтверждения identity, что уже
  используется в `desktop-ipc`), с командами `launch`, `stop` и подпиской на
  health events. Renderer к этому каналу доступа не имеет.
- Аутентифицировать каждый запрос короткоживущей provider-session capability,
  выданной supervisor через этот канал; одного loopback недостаточно. Это
  opaque 32-byte random bearer token, представленный как
  `Authorization: Bearer <base64url>`, с audience `local-provider`,
  привязкой к `request_id` и одноразовым использованием. Supervisor хранит
  только SHA-256 hash токена и сравнивает его constant-time; Core передаёт
  токен только адаптеру; адаптер проверяет hash, expiry, audience и request
  binding. JWT, ключи в renderer, prompt, command line, environment после
  запуска и логах не используются.
- TTL токена — 30 секунд **до первого предъявления**, а не на весь запрос.
  Иначе streaming-ответ, законно идущий дольше TTL, отваливался бы как
  подделка. После успешной проверки токен погашается, а авторизованной
  считается установленная сессия до её закрытия по timeout, отмене или
  завершению ответа. Startup probe получает собственный токен: одноразовость
  означает, что probe и первый запрос не могут делить одну capability.
- Все проверки TTL и expiry выполняются от `now_ms`, переданного Core (правило
  02.1: ни один компонент не читает системные часы сам). Просроченный,
  повторно использованный или чужой токен даёт `provider_session_invalid` —
  без retry и без success.
- Проверять capabilities одним startup probe по контракту 02.1: не более одной
  попытки, connect timeout 2 s, total timeout 10 s, request ≤ 4 KiB,
  response ≤ 64 KiB, без prompt пользователя и без tool side effects. Повторный
  probe перед каждым запросом не выполняется: повторное подтверждение
  происходит при новом запуске процесса или при изменении `capability_epoch`.
- Capability metadata локального провайдера — это метаданные 02.1
  (`schema_version`, `provider_version`, `capability_epoch`, `tool_calling`,
  `structured_output`, `context_limit`, `streaming`, `vision`,
  `execution_class = local`, `privacy_boundary`), дополненные local-секцией:
  `model_id`, `cancellation` и `tools[]` (имя, version, JSON Schema аргументов,
  `requires_approval`). Своей «канонической схемы» этап не вводит. Неизвестная
  major-версия, дубликаты имён инструментов, отсутствующая схема аргументов и
  несоответствие allowlist дают `capability_probe_failed` и не дают `ready`.
- Валидировать tool call до отправки: обязательны строковые `id` и `name` и
  JSON-объект `arguments`; имя должно присутствовать в capabilities, arguments
  должны проходить указанную JSON Schema, а mutation tool — иметь approval
  metadata. Невалидный запрос возвращается как `tool_call_malformed` с
  семантикой HTTP 400. Невалидный ответ модели возвращается как
  `malformed_response` (категория отказа 02.1, не собственный синоним) и
  никогда не считается fallback success.
- Не передавать local route provider secrets: cloud API keys, `Authorization`
  cloud-провайдеров и содержимое `shell/provider.json` в запрос локальной
  модели не попадают. Это граница из 02.0, и она проверяется тестом.
- Поддержать graceful absence: если local model не установлена, сообщать
  `unavailable`, не маскировать это как provider success. Безопасные причины:
  `local_model_not_found`, `capability_probe_failed`,
  `provider_session_invalid`, `loopback_policy_violation`, `port_unavailable`,
  `provider_process_exited`, `resource_limit_exceeded`, `timeout`, `cancelled`
  и `malformed_response`. Пути, токены и командные строки в причине не
  раскрываются. Список закрытый: причина, не попавшая в него, — дефект, а не
  повод для свободного текста.

### Процесс модели и его лимиты

Процесс модели — долгоживущий, один на `model_id`, а не один на запрос:
загрузка весов на каждый запрос сделала бы local route бесполезным. `launch`
keyed by `request_id` означает, что запрос-инициатор запускает процесс и
получает capability; последующие запросы к тому же `model_id` переиспользуют
уже готовый процесс, увеличивая счётчик ссылок и получая собственные токены.
Процесс останавливается, когда счётчик обнулился и истёк idle timeout
(по умолчанию 300 s), либо по явному `stop`, либо при завершении Core.

Одновременность ограничена: `max_concurrent_requests` по умолчанию 1 (один
runtime SLM не обслуживает параллельные запросы предсказуемо). Превышение
даёт не очередь без границ, а bounded ожидание не дольше connect timeout и
затем `unavailable` с причиной `timeout`.

Лимиты приходят из typed supervisor config, проходят upper bounds и
фиксируются в health event; renderer их не меняет. Различаются два процесса:

| Процесс | Memory limit | CPU limit | Upper bound |
| --- | --- | --- | --- |
| adapter shim | 512 MiB | 25% системной ёмкости | 1 GiB |
| model runtime | 4 GiB | 75% системной ёмкости | 12 GiB |

Единый лимит 512 MiB для обоих запрещён: под ним не поднимется ни один
реальный SLM runtime, и этап получил бы вечный `unavailable`, выдаваемый за
корректную работу. Итоговые значения для конкретного runtime фиксирует ADR из
02.0; здесь фиксируется механизм и порядок величин.

CPU limit задаётся так, как его умеет Windows: `JOBOBJECT_CPU_RATE_CONTROL_
INFORMATION.CpuRate` в сотых долях процента **общей** ёмкости системы, а не
«процент одного ядра» — на 8-ядерной машине «50% одного ядра» превратилось бы
в 6.25% и означало бы совсем другой лимит. Memory limit — `JOB_OBJECT_LIMIT_
JOB_MEMORY` в отдельном Job Object процесса модели; Job Object Core не
переиспользуется. Убийство по лимиту памяти отдаёт `resource_limit_exceeded`,
а не `timeout`.

Тайминги: connect timeout 2 s, read-idle timeout 10 s, cancellation grace
period 2 s, total request timeout 30 s.

Фактический дедлайн попытки — `min(total_request_timeout, остаток
run_budget)`. Без этого правила local attempt в 30 s противоречил бы
`retry.max_elapsed_ms = 15000` из 02.1: одна попытка съедала бы весь run и
делала бы retry policy декларацией. Значение 30 s остаётся верхней границей
для вызовов вне run (диагностика, warm-up, probe-free health check).

### Supervisor lifecycle и loopback

Supervisor остаётся владельцем процесса local provider: Core отправляет ему
валидированный, allowlisted launch request с `model_id` и immutable config
profile. Произвольный порт, путь или command line от renderer Core не
принимает и дальше не передаёт.

Порт выбирает supervisor, и выбирает его тем, что **сам держит слушающий
сокет** на `127.0.0.1` и передаёт его дочернему процессу, либо — если
выбранный launcher так не умеет — пробует до 8 портов подряд с
`SO_EXCLUSIVEADDRUSE` и повторяет запуск при неудаче bind. Схема «проверить
bind, отпустить, сказать порт ребёнку» запрещена как TOCTOU: диапазон
`49152-49252` целиком лежит внутри Windows dynamic port range, и порт между
проверкой и запуском занимает кто угодно. Исчерпание попыток даёт
`port_unavailable`, а не тихий выбор другого адреса.

Процесс создаётся в собственном Job Object с `KILL_ON_JOB_CLOSE`, memory и CPU
лимитами из таблицы выше. Launch/stop — keyed by `request_id`; stop до
завершения launch помечает request cancelled, а завершившийся launch
немедленно закрывается. Повторный stop идемпотентен и возвращает
`already_cancelled`.

Каждый запрос имеет bounded connect/read/total timeout. При timeout или
cancellation Core отправляет supervisor stop request; supervisor закрывает
pipe и завершает process tree через Job Object, а после grace period применяет
принудительное завершение. Отмена по authenticated каналу идемпотентна.

Проверка loopback принимает только IPv4 `127.0.0.0/8` и IPv6 `::1`, запрещает
redirect, proxy и внешний DNS и тестируется по фактическим socket
destinations, а не по строке конфигурации. Bind вне loopback, redirect и
внешний адрес дают `loopback_policy_violation`.

### Health contract

Наружу публикуются два разных измерения, и смешивать их нельзя.

`process_state` — состояние процесса у supervisor: `starting`, `running`,
`stopping`, `stopped`. `health.status` — словарь 02.1 и только он: `ready`,
`degraded`, `stale`, `unavailable`. Отображение фиксировано:

| `process_state` | причина | `health.status` |
| --- | --- | --- |
| `starting` | — | `unavailable` |
| `running` | probe пройден | `ready` |
| `running` | probe пройден, но лимиты близки к порогу или был отказ | `degraded` |
| `running` | probe не пройден | `unavailable` |
| `stopping`/`stopped` | любая | `unavailable` |

Пока процесс поднимается, route не выбирается: `starting`, поданный как
отдельный «почти ready», позволил бы 02.3 отправить запрос в ещё не готовый
процесс. Каждый переход публикуется как redacted health event с
`process_state`, `health.status`, safe reason, `request_id`, `model_id`,
`capability_epoch` и длительностями.

### Границы этапа в routing

02.2 не принимает routing-решения и не выбирает cloud provider. Классификацию
sensitivity/offline выполняет Core-owned classifier (02.0), а policy выбора и
разрешение fallback принадлежат 02.3. От 02.2 требуется ровно одно: статус и
причина, на которых 02.3 может построить решение, и отсутствие скрытого
retry или подмены результата.

Для sensitive/offline задачи `unavailable` обязан оставаться пригодным для
bounded refusal: адаптер не пробует cloud, не ретраится внутри себя и не
возвращает пустой success. Существующее поведение `LocalFirst` (снятие
фильтра и выбор cloud с `fallback.reason = local_route_unavailable`) этап не
изменяет и не расширяет; ограничение этого перехода privacy policy —
работа 02.3.

## Проверки

- malformed tool-call tests: невалидный JSON, неизвестное имя, отсутствие
  `id/name/arguments`, нарушение JSON Schema и mutation без approval дают
  `tool_call_malformed`/400; malformed model response даёт `malformed_response`
  и не засчитывается как успешный fallback;
- local unavailable → `unavailable` с каждой из безопасных причин, включая
  `provider_process_exited`, `resource_limit_exceeded` и `port_unavailable`;
  причина не содержит путей, токенов и command line;
- token authentication на подставленном `now_ms`, без ожидания реального
  времени: правильный token проходит, неверный/чужой/истёкший по TTL/повторно
  использованный отвергается без retry; сравнение hash constant-time; токен,
  предъявленный вовремя, не аннулируется при ответе длиннее 30 s;
- capability probe: неизвестная major-версия, duplicate tool, отсутствующая
  JSON Schema и превышение размеров request/response не дают `ready`; probe не
  ретраится и укладывается в 10 s;
- secrets boundary: запрос к local route не содержит cloud API key,
  `Authorization` cloud-провайдера и содержимого `shell/provider.json`;
- Windows loopback и supervisor lifecycle: bind вне `127.0.0.0/8`/`::1`,
  внешний redirect, занятый порт и исчерпание диапазона, launch→stop race,
  идемпотентный повторный stop и Job Object cleanup;
- resource limits: превышение memory limit убивает процесс модели и даёт
  `resource_limit_exceeded`; Job Object процесса модели отделён от Job Object
  Core, и его закрытие не задевает Core;
- process reuse и concurrency: второй запрос к тому же `model_id` не
  перезапускает процесс, счётчик ссылок и idle timeout останавливают его ровно
  один раз, а превышение `max_concurrent_requests` даёт bounded ожидание и
  `timeout`, а не неограниченную очередь;
- streaming/cancellation: отмена во время stream завершает запрос не дольше чем
  за grace period + 3 s, публикует `cancelled`, закрывает pipe и не оставляет
  child process;
- deadline: при остатке run budget меньше `total_request_timeout` попытка
  прерывается по остатку, а не по 30 s;
- health mapping: каждый `process_state` даёт ровно тот `health.status`, что в
  таблице; `starting` наружу не выглядит доступным;
- routing compatibility: регистрация local route не ломает существующие тесты
  `LocalFirst`; `fallback.reason = local_route_unavailable` остаётся тем же
  значением с тем же смыслом, а `execution_class = local` совпадает с тем, что
  сегодня определяет подстрочный `is_local_route`;
- observability: для каждого lifecycle перехода есть redacted health event, а
  секреты, prompt и command line отсутствуют в JSONL logs.

## Критерии готовности

- отсутствие локальной модели видно как `unavailable` с закрытым списком
  безопасных причин;
- local route не выходит за loopback и подчиняется supervisor lifetime;
- канал команд `Core → supervisor` существует, аутентифицирован и недоступен
  renderer; supervisor управляет вторым дочерним процессом, не теряя контроля
  над Core;
- capability metadata локального провайдера — это метаданные 02.1 с local-
  секцией, а не параллельная схема; `capability_epoch` меняется при изменении
  набора инструментов;
- ни одна попытка не выходит за `min(total_request_timeout, остаток
  run_budget)`, а malformed response, timeout и cancellation никогда не
  маскируются под success;
- лимиты памяти и CPU выражены средствами Job Object и различают adapter shim
  и model runtime;
- authenticated session token, capability schema, лимиты и state/error
  contract покрыты acceptance tests, включая concurrent requests, повторное
  использование процесса и launch/stop race;
- `ProviderKind`/`ModelProvider` расширены вариантом `Local`, capability и
  cancellation; существующее поведение `local_route_unavailable` сохранено без
  изменений и передано 02.3 как есть.
