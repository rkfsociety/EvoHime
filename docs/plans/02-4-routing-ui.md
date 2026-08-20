# Этап 02.4: UI

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 02.3 — UI строго потребляет trace schema, определённую там
(`schema_version = 1`, `selected_route`, `reason_code`, `terminal_status`,
`safe_next_action`, `candidates[]` с `health_state`/`reject_reason`,
`fallback_count`, `privacy_label`). UI не вводит собственных кодов и не
переопределяет то, что уже зафиксировано в 02.3.

**Frozen constraint.** Это блокирующее предусловие этапа: перед началом
реализации 02.4 этап 02.3 должен опубликовать под `schema_version = 1` полный
список значений
`terminal_status`/`safe_next_action`/`health_state`/`reason_code`/
`reject_reason`/`privacy_label`. Любое расширение этих перечислений —
это новый `schema_version`, требующий согласованного обновления localization
table в 02.4 (см. «Локализация»); молчаливое изменение набора значений под тем
же `schema_version` запрещено.

### Что 02.3 фиксирует для UI

Пять вещей, без которых UI-контракт неполон, закреплены в 02.3 (раздел
«Закрытые перечисления schema v1» и «Что доходит до renderer»), и 02.4 берёт
их оттуда без переопределения:

1. **Успешное значение `terminal_status`** — `success`. Отдельно существует
   `cancelled` (внешняя отмена). UI никогда не опирается на «поле
   отсутствует».
2. **`privacy_label`** — обязательное поле trace, закрытое перечисление
   `sensitive` / `non_sensitive` / `unknown`. Критерий degraded mode ветвится
   именно на нём.
3. **`reason_code` и `reject_reason`** опубликованы как закрытые перечисления
   (17 и 18 значений соответственно) — на них опирается build-time проверка
   полноты localization table.
4. **`selected_route`** — `route_id` при `success`, строго `null` при любом
   другом `terminal_status`. Отсутствие ключа — malformed payload.
5. **Что доходит до UI** — терминальная запись run (последняя по `sequence`
   для `trace_id`, с непустым `terminal_status`) плюс отдельное нетерминальное
   IPC-событие `pending_approval`. Поток промежуточных записей renderer не
   получает; они доступны только diagnostics view по `trace_id`.

`refusal-family` (используется ниже в критериях) — полный набор
`terminal_status` из таблиц `RouteError` и «Состояния, возникающие вне
`select_route`» в 02.3: `no_routes_configured`, `both_routes_unavailable`,
`classification_incomplete`, `context_limit_exceeded`, `policy_violation`,
`budget_unavailable`, `context_assembly_failed`, `fallback_limit_reached`,
`run_deadline_exceeded`, `reroute_approval_declined`, `internal_error`.
`success` и `cancelled` в семейство не входят и рендерятся отдельно.
`pending_approval` — не `terminal_status`, а отдельное IPC-событие; UI не
рендерит его как отказ (см. состояние 3 ниже).

Это последний этап плана.

## Что этап отдаёт наружу

Честное отображение фактического route и причин отказа, целиком выводимое из
Core trace — без интерпретации или домысливания на стороне renderer.

## Контракт с Core (вход)

UI получает только то, что Core отдаёт через IPC как терминальную запись
decision/trace (см. «Формат trace и наблюдаемость» и «Что доходит до
renderer» в [02-3](02-3-routing-and-budget.md)).

**Обязательные поля** (отсутствие любого из них — malformed payload):
`schema_version`, `terminal_status`, `reason_code`, `selected_route`,
`candidates[]`, `fallback_count`, `privacy_label`, `trace_id`, `run_id`,
`sequence`, а также `safe_next_action` (при `terminal_status ∈
refusal-family`). `selected_route` обязателен как ключ всегда: при
`success` — `route_id`, при любом другом статусе — `null`. Ключ отсутствует
или `null` при `success` — malformed payload; непустой `selected_route` при
статусе из refusal-family разбирается по правилу приоритета ниже.

**Поля, используемые только в diagnostics view:** `attempt_id`, `now_ms`,
`policy_version`, `catalog_version`, `snapshot_hash`, `latency_ms`, `usage`,
`event`, `classification`, бюджетная часть (`budget_id`/`budget_absent`,
`estimated_input_tokens`, `profile_version`, `context_ledger_hash`), а также
исходные `health_status` и `circuit_state` кандидатов. Их отсутствие не
влияет на основной UX и не считается malformed.

Каждый `candidates[]` элемент обязан содержать `route_id` и производный
`health_state` ∈ `{healthy, degraded, unavailable}`; `reject_reason`
опционален (присутствует, когда candidate был отклонён — включая preferred
route пользователя). UI ветвится только на `health_state`: смешивать
`health_status` и `circuit_state` запрещено 02.3, и renderer не воспроизводит
таблицу проекции — он получает уже вычисленный результат.

`fallback_count` используется ровно в двух местах: в тексте partial fallback
(когда `> 0` — «использован резервный маршрут») и в diagnostics view. На
выбор визуального состояния он не влияет: состояние определяется
`terminal_status`/`selected_route`/`preferred_route_hint`.

**Malformed/missing data.** Если обязательное поле отсутствует, имеет
неверный тип или payload не проходит schema validation — UI не пытается
частично отобразить его и не показывает route. Поведение идентично
`core_unavailable` (см. ниже): блокирует отправку задач, требующих routing
decision, предлагает retry, логирует `malformed_trace_payload` с исходным
payload в diagnostics. Дополнительной редакции на стороне renderer не
требуется: по контракту 02.3 trace не содержит prompt, token text, ключей и
raw model output.

**Несовместимый `schema_version`.** Если major-часть `schema_version` в
payload не совпадает с той, под которую собран UI (сейчас `1`), семантика
полей не гарантирована — UI не разбирает payload по полям: это
`core_unavailable` с диагностическим событием
`unsupported_schema_version=<version>`. Совпадение major при большем minor
считается совместимым: payload разбирается, а незнакомые значения
перечислений попадают в `unknown_state` (см. «Локализация»).

**Приоритет валидации** (первое сработавшее правило выигрывает):
несовместимый `schema_version` → структурная невалидность payload →
незнакомое значение перечисления → нормальный разбор. Внутри разобранного
payload `terminal_status` главнее `selected_route`: если статус принадлежит
`refusal-family`, UI рендерит отказ, даже если `selected_route` непустой.

`terminal_status`, `safe_next_action`, `health_state`, `reason_code`,
`reject_reason` и `privacy_label` — закрытые перечисления, зафиксированные в
02.3 под конкретным `schema_version`. UI хранит **таблицу локализации**
каждого значения в человекочитаемый текст; сырые коды никогда не показываются
пользователю напрямую (доступны только в debug/diagnostics view).

## Локализация

- Localization table версионируется вместе с `schema_version` trace schema:
  каждая запись таблицы явно привязана к диапазону `schema_version`, для
  которого она валидна. Обновление localization table — часть того же PR/
  release, что расширяет перечисления в 02.3; CI проверяет соответствие
  набора ключей localization table полному списку значений enum для текущего
  `schema_version` (build-time проверка полноты, не runtime).
- Покрытие обязательно для `terminal_status`, `safe_next_action`,
  `health_state`, `reason_code` и `reject_reason`. `health_status`,
  `circuit_state` и бюджетные поля в основном UX не показываются — они
  живут в diagnostics view, где сырые коды допустимы.
- Минимум один язык интерфейса поддерживается на старте (текущий язык UI);
  расширение на другие языки — независимая доработка таблицы, не блокирует
  готовность этапа.
- Если IPC возвращает значение вне известного перечисления для активного
  `schema_version` — это отдельный визуальный шаблон `unknown_state`, не
  совпадающий ни с шаблоном отказа, ни с `core_unavailable`:
  - `safe_next_action` принудительно `contact_support`, независимо от того,
    что могло прийти в trace;
  - UI логирует диагностическое событие `unsupported_enum=<field>=<code>`
    (raw-код сохраняется только в лог/diagnostics, не в основной UX);
  - `unknown_state` не пытается угадать смысл кода и не подставляет ближайшее
    известное значение.
- Обновление таблицы поставляется вместе со сборкой UI (не OTA/runtime-fetch):
  таблица — часть релиза, синхронизированного с версией Core по
  `schema_version`, что исключает рассинхронизацию между новым Core и старым
  UI без явного релиза.

## Визуальная спецификация

Состояния разделены на два слоя: **основное состояние ответа** (ровно одно
активно на экране в любой момент) и **вторичные аннотации** (могут
сопровождать основное состояние, не заменяя его).

### Основные состояния и порядок разрешения

Условия состояний пересекаются (degraded — строгое подмножество partial
fallback), поэтому взаимоисключающими их делает фиксированный порядок: первое
сработавшее правило и есть состояние экрана.

| # | Состояние | Условие |
| --- | --- | --- |
| 1 | `core_unavailable` | транспортная ошибка IPC, malformed payload или несовместимый major `schema_version` |
| 2 | `unknown_state` | payload структурно валиден, но содержит значение вне известного перечисления |
| 3 | `pending_approval` | получено нетерминальное IPC-событие подтверждения re-routing |
| 4 | Cancelled | `terminal_status = cancelled` |
| 5 | Refusal | `terminal_status ∈ refusal-family` |
| 6 | Degraded | критерий degraded mode (см. «Поведение UI») |
| 7 | Partial fallback | `preferred_route_hint != nil AND selected_route != preferred_route_hint AND terminal_status = success` |
| 8 | Normal | всё остальное |

1. **`core_unavailable`** — блокирующий баннер поверх области ответа с
   кнопкой retry; route selector в этом состоянии неактивен (disabled, не
   просто visually dimmed) и не показывает предыдущий route как актуальный.
2. **`unknown_state`** — нейтральный шаблон «действие недоступно, обратитесь
   в поддержку», без попытки показать raw enum пользователю.
3. **`pending_approval`** — запрос подтверждения перехода на cloud route
   после post-analysis re-routing (02.3). Не отказ и не доставленный ответ:
   две явные кнопки (подтвердить / отклонить), видимый обратный отсчёт до
   `expires_at_ms` из самого события (Core вычисляет его как `now_ms +
   routing.reroute_approval_timeout_ms`, 120 с по 02.3 — UI не считает
   дедлайн сам и не берёт значение таймаута из своей сборки) и указание, какой
   именно route предлагается. По истечении таймаута UI не решает за
   пользователя и не отправляет подтверждение автоматически: он показывает,
   что время вышло, и ждёт терминальный trace от Core с
   `reroute_approval_declined`, который переводит экран в состояние Refusal.
4. **Cancelled** — нейтральное сообщение «запрос отменён», без причины отказа
   и без `safe_next_action` (по 02.3 он `null` для этого статуса). Это не
   отказ маршрутизации: отмену инициировали снаружи, и объяснять её
   пользователю как сбой нельзя.
5. **Refusal** — общий шаблон «причина + `safe_next_action`», текст причины
   берётся из localization table по `terminal_status`/`reason_code`.
   Отдельный визуальный подтип на каждый статус не заводится: шаблон один,
   различается наполнение. Обязательное требование — `no_routes_configured`
   («маршруты не настроены») и `both_routes_unavailable` («все настроенные
   маршруты сейчас недоступны») имеют разный текст: 02.3 прямо запрещает
   смешивать отсутствие конфигурации с отказами провайдеров.
6. **Degraded** — индикатор `⚠ Degraded` в заголовке ответа + краткая причина
   (человекочитаемый текст из `reason_code`, не сам код). Не сворачивается
   автоматически; закрывается только явным действием пользователя
   (dismiss-контрол в самом индикаторе), при следующем ответе состояние
   пересчитывается заново из нового trace.
7. **Partial fallback** — сообщение «ответ получен не через предпочитаемый
   маршрут» с указанием фактического `selected_route`. Это не отказ: ответ
   доставлен. Сюда же попадает случай, когда предпочтение было `local`, а
   фактически отработал `cloud` — умалчивать об этом нельзя, случай
   privacy-значимый.
8. **Normal** — route selector отображает `selected_route`, никаких
   предупреждений.

### Вторичные аннотации

- **Отказ конкретного route.** Если у candidate есть `reject_reason` (в
  первую очередь у preferred route пользователя) — его человекочитаемый текст
  показывается рядом с route selector, отдельно от основного состояния и от
  общего terminal reason. Аннотация может сопровождать любое основное
  состояние из 5–8 и не заменяет его собой. При
  `core_unavailable`/`unknown_state` аннотации не показываются: доверять
  содержимому payload в этих состояниях нельзя.

**Route selector (preferred route hint).** Отдельный, всегда доступный
контрол (кроме состояния `core_unavailable`, где он disabled) — dropdown/список
доступных route. Список опций — **не** `candidates[]` (он меняется от run к
run и при `no_routes_configured` пуст), а перечень route id, зафиксированный
для сборки вместе с `schema_version`, плюс опция «без предпочтения» (`nil`).
Если сохранённый hint отсутствует в перечне текущей сборки, UI сбрасывает его
в `nil` и логирует `unknown_preferred_route=<id>` — молча подставлять другой
route запрещено. Выбор пользователя:
- сохраняется как client-side настройка немедленно при изменении (локально,
  без отдельного подтверждения — это hint, не команда);
- передаётся в Core при следующем запросе как часть `RoutingRequest`;
- не меняет отображаемый результат текущего уже отрендеренного ответа —
  изменение вступает в силу со следующего запроса.
Retry на `core_unavailable` — кнопка в баннере, инициирующая новый IPC-запрос
статуса Core; не трогает route selector.

## Поведение UI

- Показывать фактически выбранный `selected_route`, а не желаемый.
- Состояния отказа различаются напрямую по `terminal_status` (не
  придумываются UI). Partial fallback — это НЕ `terminal_status`, а
  производное состояние (формула — в таблице выше): ответ успешно доставлен,
  но не через предпочитаемый маршрут. Поскольку терминальная запись trace
  несёт ровно один `terminal_status`, проблема приоритизации нескольких
  одновременных terminal-причин не возникает: Core уже выбрал единственный
  статус до передачи в UI. Пересечение производных состояний
  (degraded ⊂ partial fallback) разрешается порядком из таблицы, а не
  эвристикой. Если у пользовательского preferred route есть собственный
  `reject_reason` в `candidates[]`, он показывается вторичной аннотацией как
  объяснение, почему предпочтение не сработало — отдельно от terminal reason
  всего run.
- **Preferred route** хранится как непривилегированная client-side настройка
  и передаётся в Core как *hint* в `RoutingRequest`, не как команда. В
  порядке правил 02.3 user preference стоит предпоследним, перед лексическим
  tie-break по `route_id`, — то есть перекрывается любым фильтром политики,
  health/circuit, gate и бюджета. Core решает маршрут независимо от hint;
  результат для preferred route виден в `candidates[]` через его
  `health_state`/`reject_reason`. UI не имеет пути применить preferred route
  в обход возвращённого `selected_route` — селектор пишет только в
  настройку-hint и никогда не подменяет отображаемый результат. Это гарантия
  by construction: рендер результата read-only и всегда идёт из последнего
  Core trace, а не из локального состояния выбора пользователя.
  - При первом запуске (`preferred_route_hint` ещё не установлен, значение
    `nil`/отсутствует) UI не передаёт hint вовсе (Core трактует отсутствие
    hint как «нет предпочтения», не как «local»/«cloud» по умолчанию);
    degraded mode и partial fallback критерии, зависящие от
    `preferred_route_hint`, автоматически не срабатывают, пока hint не задан
    явно пользователем.
- **Degraded mode** — точный критерий активации:
  ```
  preferred_route_hint != nil AND
  selected_route == local AND
  selected_route != preferred_route_hint AND
  terminal_status == success AND
  privacy_label == non_sensitive
  ```
  (`privacy_label` — Core-owned значение из trace, UI только ветвится на нём.)
  Если `preferred_route_hint == nil`, degraded mode не активируется — нет
  предпочтения, значит нет отклонения от него. Если критерий выполнен не
  полностью, но выполнено условие partial fallback, показывается partial
  fallback (состояние 7), а не normal. Визуальный маркер и поведение — см.
  «Визуальная спецификация» выше.
- **Недоступность Core IPC** — отдельное, нетерминальное UI-состояние (не
  `terminal_status`, а транспортная ошибка): UI показывает
  `core_unavailable`, блокирует отправку задач, требующих routing decision, и
  предлагает retry. Не показывает route selector как активный и не
  подставляет предыдущий известный route.
- Локализация: таблица кодов → текст поддерживает как минимум текущий язык
  интерфейса; текст не содержит внутренних имён кодов (например,
  `policy_violation` → «Правила безопасности не позволяют использовать этот
  маршрут»).

## Граничные случаи

- **Пустой `candidates[]`** при `terminal_status = success`: допустимо
  (Core мог не передать полный список кандидатов для успешного пути) — UI
  рендерит normal/degraded/partial fallback по `selected_route`, но вторичная
  аннотация о `reject_reason` preferred route показана быть не может — UI
  молча пропускает этот под-текст, не подставляет заглушку.
- **Пустой `candidates[]` при отказе.** Непустой список обязателен только для
  статусов, которые по определению 02.3 возникают после перебора кандидатов:
  `both_routes_unavailable` (`all_routes_excluded`), `context_limit_exceeded`
  и `context_assembly_failed`; пустой список для них — malformed payload.
  Наоборот, `no_routes_configured` (snapshot пуст) и `budget_unavailable`
  (selection не запускалась) приходят с пустым `candidates[]` **штатно** — это
  обычный refusal. Для остальных статусов пустой список допустим и не влияет
  на рендер основного состояния.
- **`preferred_route_hint` не задан** (первый запуск) — см. пункт выше в
  «Поведение UI»: hint не передаётся, degraded/partial fallback не
  вычисляются.
- **Race: терминальный trace пришёл одновременно с IPC-ошибкой транспорта** —
  транспортный уровень обрабатывается отдельно от парсинга payload: если
  соединение оборвалось до получения полной записи, это `core_unavailable`
  независимо от того, что успело прийти частично. Если полная валидная запись
  получена и только после этого соединение изменило статус — уже
  отрендеренный trace не откатывается; `core_unavailable` относится только к
  следующему запросу.
- **Разрыв IPC во время `pending_approval`** — подтверждение считается
  неотправленным: экран переходит в `core_unavailable`, а решение о судьбе
  run остаётся за Core (таймаут 02.3 приведёт к
  `reroute_approval_declined`). UI не досылает подтверждение после
  восстановления связи.

## Доступность (accessibility)

- Все индикаторы состояния (degraded, partial fallback, refusal,
  `pending_approval`, `unknown_state`, `core_unavailable`) имеют ARIA
  live-region, так что screen reader озвучивает смену состояния без
  необходимости фокуса: `role="alert"` для блокирующих и отказных состояний
  (`core_unavailable`, `unknown_state`, refusal, `pending_approval`),
  `role="status"` для неблокирующих (degraded, partial fallback, вторичные
  аннотации).
- Обратный отсчёт `pending_approval` не озвучивается на каждый тик: live-
  region обновляется на переходах (появление запроса, истечение таймаута),
  иначе screen reader забивает канал.
- Текст в каждом состоянии соответствует минимальному контрасту WCAG 2.1 AA
  (4.5:1 для обычного текста), включая `⚠ Degraded` индикатор и баннер
  `core_unavailable`.
- Route selector, кнопки `pending_approval`, retry-контрол на
  `core_unavailable` и dismiss-контрол degraded-индикатора полностью
  управляемы с клавиатуры (tab-order, Enter/Space активация), без mouse-only
  путей.
- Diagnostics view (raw `trace_id`/коды) доступен через тот же
  keyboard-navigable путь, что и остальной UI — не требует mouse-only
  контекстного меню.

## Тестовая стратегия

- **Unit:** localization table — build-time проверка, что набор ключей
  покрывает 100% значений `terminal_status`/`safe_next_action`/`health_state`/
  `reason_code`/`reject_reason` для текущего `schema_version` (см.
  «Локализация»); unit-тест на `unknown_state` fallback при неизвестном
  значении и на malformed payload (отсутствующее обязательное поле, неверный
  тип) → `core_unavailable`-путь; unit-тест на несовместимый major
  `schema_version` → `core_unavailable`, а не разбор по полям.
- **Unit: порядок разрешения состояний** — таблица приоритетов проверяется на
  конфликтных payload: несовместимый `schema_version` + валидный
  `terminal_status`; неизвестный enum + refusal-статус; refusal-статус +
  непустой `selected_route`; одновременно выполненные условия degraded и
  partial fallback.
- **Integration (мок Core IPC):** сценарии транспорта — success (валидная
  терминальная запись), timeout (нет ответа в срок), malformed (частичный/
  повреждённый payload), `pending_approval` с подтверждением, с отклонением и
  с истечением таймаута — каждый проверяется на итоговое визуальное
  состояние.
- **Матрица производных состояний** (обязательна к покрытию; `success` и
  `cancelled` — значения `terminal_status` вне refusal-family, 02.3):

  | `preferred_route_hint` | `selected_route` | `terminal_status` | `privacy_label` | Ожидаемое состояние |
  | --- | --- | --- | --- | --- |
  | nil | local | success | non_sensitive | normal (не degraded — нет hint) |
  | cloud | local | success | non_sensitive | degraded |
  | cloud | cloud | success | non_sensitive | normal |
  | cloud | local | success | sensitive | partial fallback, не degraded |
  | local | cloud | success | non_sensitive | partial fallback (предпочтение local не соблюдено) |
  | local | local | success | non_sensitive | normal (hint совпадает с выбором) |
  | cloud | — | `both_routes_unavailable`, непустой `candidates[]` | non_sensitive | refusal, не degraded и не partial fallback |
  | cloud | — | `both_routes_unavailable`, пустой `candidates[]` | non_sensitive | `core_unavailable` (malformed) |
  | cloud | — | `no_routes_configured`, пустой `candidates[]` | non_sensitive | refusal (текст отличается от `both_routes_unavailable`) |
  | cloud | — | `budget_unavailable`, пустой `candidates[]` | non_sensitive | refusal, не malformed |
  | cloud | — | `cancelled` | non_sensitive | cancelled, не refusal и не partial fallback |

- **E2E:** полный проход по всем восьми основным состояниям (normal, partial
  fallback, degraded, refusal, cancelled, `pending_approval`, `unknown_state`,
  `core_unavailable`) плюс вторичная аннотация `reject_reason` у preferred
  route — на реальном (не мокнутом) IPC-канале в staging-конфигурации Core.
  Refusal проверяется как минимум на паре `no_routes_configured` /
  `both_routes_unavailable`, чтобы подтвердить их различимость.

## Проверки

- refusal, partial fallback и аннотация `reject_reason` конкретного route
  различимы в UI и не сливаются в одно сообщение;
- `no_routes_configured` и `both_routes_unavailable` дают разный текст, как
  того требует 02.3;
- порядок разрешения состояний детерминирован: при одновременно выполненных
  условиях экран показывает состояние с меньшим номером в таблице —
  проверено unit-тестом;
- preferred route hint при конфликте не меняет отображаемый `selected_route`
  — UI показывает Core-возвращённое решение, а `reject_reason` preferred
  route виден отдельной аннотацией;
- UI показывает фактический, а не желаемый route после fallback, включая
  случай «предпочтение local, фактически cloud»;
- degraded mode активируется и гаснет строго по определённому условию выше
  (включая `preferred_route_hint == nil` → никогда не активируется),
  проверено на матрице из «Тестовой стратегии»;
- при недоступности Core IPC UI показывает `core_unavailable`, не пытается
  показать предыдущий route как актуальный;
- malformed/missing обязательное поле и несовместимый major `schema_version`
  обрабатываются как `core_unavailable`, а не частичный рендер;
- неизвестное/будущее значение enum от Core не роняет UI и не рендерится как
  raw-код — попадает в `unknown_state` fallback с `safe_next_action =
  contact_support` и логированием `unsupported_enum`;
- localization table покрывает все значения `terminal_status`/
  `safe_next_action`/`health_state`/`reason_code`/`reject_reason` для текущего
  `schema_version` — проверено build-time;
- `pending_approval` показывает обе кнопки и таймаут, не подтверждает переход
  автоматически ни по истечении времени, ни при разрыве связи;
- `cancelled` рендерится как отдельное нейтральное состояние: без
  `safe_next_action`, без текста отказа и без partial fallback;
- renderer получает только терминальную запись и событие `pending_approval`:
  поток промежуточных attempt-записей в основной UX не попадает;
- пустой `candidates[]` считается ошибкой только для
  `both_routes_unavailable`/`context_limit_exceeded`/`context_assembly_failed`;
  для `no_routes_configured` и `budget_unavailable` это штатный случай;
- список опций route selector берётся из зафиксированного перечня сборки, а
  не из `candidates[]`; неизвестный сохранённый hint сбрасывается в `nil` с
  логированием, а не подменяется другим route;
- accessibility: ARIA live-region нужной срочности на смену состояния,
  контраст AA, полная keyboard-навигация route selector, кнопок
  `pending_approval`, retry и dismiss.

## Критерии готовности

- 02.3 опубликовал полный набор значений перечислений под
  `schema_version = 1` и контракт доставки в renderer; UI не начинает
  реализацию до этой фиксации (frozen constraint выше);
- UI показывает фактический результат routing, выведенный напрямую из
  терминальной записи Core trace (без собственной интерпретации/новых кодов);
- cloud outage оставляет usable local degraded mode при выполнении точного
  критерия degraded mode, если он настроен;
- preferred route hint не может изменить `selected_route` — механизм
  read-only рендера описан и покрыт тестом;
- refusal, partial fallback, `pending_approval`, `unknown_state` и degraded
  mode понятны без просмотра технического trace — localization table
  покрывает весь закрытый набор enum-значений из 02.3 и проверяется
  build-time на полноту;
- sensitive/offline refusal показывает безопасное следующее действие
  (`safe_next_action`) как есть из trace, без собственной эвристики;
- недоступность Core IPC, malformed trace payload и несовместимый
  `schema_version` обработаны как одно общее нетерминальное состояние, не как
  один из `terminal_status`;
- визуальная спецификация (восемь основных состояний + вторичные аннотации +
  route selector) и accessibility требования выполнены и покрыты E2E;
- тестовая стратегия (unit/порядок разрешения/integration/матрица/E2E)
  реализована и проходит в CI.
