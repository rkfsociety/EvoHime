# Этап 02.4: UI

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 02.3 — UI строго потребляет trace schema, определённую там
(`schema_version`, `selected_route`, `reason_code`, `terminal_status`,
`safe_next_action`, `candidates[]` с `health_state`/`reject_reason`,
`fallback_count`, `privacy_label`). UI не вводит собственных кодов и не
переопределяет то, что уже зафиксировано в 02.3.

**Frozen constraint.** Разработка UI начинается только после того, как 02.3
зафиксировал конкретный `schema_version` и опубликовал полный список
значений `reason_code`/`terminal_status`/`safe_next_action`/`health_state`
для этой версии. Любое расширение перечислений в 02.3 после этого момента —
это новый `schema_version`, требующий согласованного обновления localization
table в 02.4 (см. «Локализация» ниже); молчаливое изменение набора значений
под тем же `schema_version` запрещено.

`truthful_refusal-family` (используется ниже в критерии degraded mode) — это
множество всех `terminal_status`, отличных от успешного выбора route:
`budget_exhausted`, `snapshot_stale`, `policy_violation`,
`internal_budget_error`, `both_routes_unavailable`,
`classification_incomplete`, `context_limit_exceeded`,
`fallback_limit_reached`, `reroute_approval_declined` (полный список — из
таблиц `BudgetError`/`RouteError`/`run_budget` в 02.3, привязан к тому же
`schema_version`).

Это последний этап плана.

## Что этап отдаёт наружу

Честное отображение фактического route и причин отказа, целиком выводимое из
Core trace — без интерпретации или домысливания на стороне renderer.

## Контракт с Core (вход)

UI получает только то, что Core отдаёт через IPC как decision/trace (см.
«Формат trace и наблюдаемость» в [02-3](02-3-routing-and-budget.md)).

**Обязательные поля** (отсутствие любого из них — malformed payload):
`schema_version`, `selected_route`, `terminal_status`, `reason_code`,
`candidates[]`, `fallback_count`, `privacy_label`, `safe_next_action` (только
при `terminal_status ∈ truthful_refusal-family`; при успешном route это поле
законно отсутствует).

**Опциональные поля:** `run_id`/`trace_id` (используются только в
diagnostics view, отсутствие не влияет на основной UX).

Каждый `candidates[]` элемент обязан содержать `route_id` и `health_state` ∈
`{healthy, degraded, unavailable}`; `reject_reason` опционален (присутствует,
когда candidate был отклонён — включая preferred route пользователя).

**Malformed/missing data.** Если обязательное поле отсутствует, имеет
неверный тип (например, `selected_route` пришёл `null` вместо строки) или не
проходит schema validation — UI не пытается частично отобразить payload и не
показывает route. Поведение идентично `core_unavailable` (см. ниже): блокирует
отправку задач, требующих routing decision, предлагает retry, логирует
`malformed_trace_payload` с исходным (redacted) payload в diagnostics.

`reason_code`, `terminal_status`, `safe_next_action` и `health_state` —
закрытые перечисления, зафиксированные в 02.3 под конкретным
`schema_version`. UI хранит **таблицу локализации** каждого значения в
человекочитаемый текст; сырые коды никогда не показываются пользователю
напрямую (доступны только в debug/diagnostics view).

## Локализация

- Localization table версионируется вместе с `schema_version` trace schema:
  каждая запись таблицы явно привязана к диапазону `schema_version`, для
  которого она валидна. Обновление localization table — часть того же PR/
  release, что расширяет перечисления в 02.3; CI проверяет соответствие
  набора ключей localization table полному списку значений enum для текущего
  `schema_version` (build-time проверка полноты, не runtime).
- Минимум один язык интерфейса поддерживается на старте (текущий язык UI);
  расширение на другие языки — независимая доработка таблицы, не блокирует
  готовность этапа.
- Если IPC возвращает значение вне известного перечисления для активного
  `schema_version` — это **четвёртый визуальный шаблон**, `unknown_state`,
  отдельный от трёх состояний отказа и от `core_unavailable`:
  - `safe_next_action` принудительно `contact_support`, независимо от того,
    что могло прийти в trace;
  - UI логирует диагностическое событие `unsupported_enum=<code>` (raw-код
    сохраняется только в лог/diagnostics, не в основной UX);
  - `unknown_state` не пытается угадать смысл кода и не подставляет ближайшее
    известное значение.
- Обновление таблицы поставляется вместе со сборкой UI (не OTA/runtime-fetch):
  таблица — часть релиза, синхронизированного с версией Core по
  `schema_version`, что исключает рассинхронизацию между новым Core и старым
  UI без явного релиза.

## Визуальная спецификация

Пять взаимоисключающих визуальных состояний ответа (ровно одно активно на
экране в любой момент):

1. **Normal** — `terminal_status` отсутствует/успешный, route selector
   отображает `selected_route`, никаких предупреждений.
2. **Degraded** — индикатор `⚠ Degraded` в заголовке ответа + краткая причина
   (человекочитаемый текст из `reason_code`, не сам код). Не сворачивается
   автоматически; закрывается только явным действием пользователя
   (dismiss-контрол в самом индикаторе), при следующем ответе состояние
   пересчитывается заново из нового trace.
3. **Refusal (три подтипа)** — каждый использует общий шаблон «причина +
   safe_next_action», но с разным текстом источника:
   - `both_routes_unavailable` — сообщение «оба маршрута недоступны» +
     `contact_support`;
   - policy-отказ конкретного route (`reject_reason` у candidate, включая
     preferred) — сообщение «этот маршрут заблокирован политикой» показывается
     рядом с route selector, отдельно от общего terminal reason;
   - partial fallback (см. определение ниже) — сообщение «использован
     резервный маршрут вместо предпочитаемого» без статуса отказа (это не
     `terminal_status`, ответ доставлен).
4. **`unknown_state`** — нейтральный шаблон «действие недоступно, обратитесь в
   поддержку», без попытки показать raw enum пользователю.
5. **`core_unavailable`** — блокирующий баннер поверх области ответа с
   кнопкой retry; route selector в этом состоянии неактивен (disabled, не
   просто visually dimmed) и не показывает предыдущий route как актуальный.

**Route selector (preferred route hint).** Отдельный, всегда доступный
контрол (кроме состояния `core_unavailable`, где он disabled) — dropdown/список
доступных route. Выбор пользователя:
- сохраняется как client-side настройка немедленно при изменении (локально,
  без отдельного подтверждения — это hint, не команда);
- передаётся в Core при следующем `prepare()`/`select_route`;
- не меняет отображаемый результат текущего уже отрендеренного ответа —
  подтверждение вступает в силу с следующего запроса.
Retry на `core_unavailable` — кнопка в баннере, инициирующая новый IPC-запрос
статуса Core; не трогает route selector.

## Поведение UI

- Показывать фактически выбранный `selected_route`, а не желаемый.
- Три состояния отказа различаются напрямую по `terminal_status` (не
  придумываются UI): `both_routes_unavailable` (ни cloud, ни local),
  policy-отказ конкретного route из candidate `reject_reason`, и **partial
  fallback** — это НЕ отдельный `terminal_status`, а производное состояние,
  вычисляемое как комбинация: `selected_route == local AND selected_route !=
  preferred_route_hint AND terminal_status ∉ truthful_refusal-family` (то
  есть ответ успешно доставлен, но не через предпочитаемый маршрут). Каждому
  назначен собственный визуальный шаблон; сообщения не сливаются в одно.
  Поскольку trace — это одно решение с одним `terminal_status` за раз,
  проблема приоритизации нескольких одновременных причин не возникает: Core
  уже выбрал единственный терминальный статус до передачи в UI. Если у
  пользовательского preferred route есть собственный `reject_reason` в
  `candidates[]`, он показывается как объяснение, почему предпочтение не
  сработало — отдельно от terminal reason всего run.
- **Preferred route** хранится как непривилегированная client-side настройка
  и передаётся в Core как *hint* при `prepare()`/`select_route`, не как
  команда. Core решает маршрут независимо от hint; результат для preferred
  route виден в `candidates[]` через его `health_state`/`reject_reason`. UI
  не имеет пути применить preferred route в обход возвращённого
  `selected_route` — селектор route в UI пишет только в настройку-hint,
  никогда не подменяет отображаемый результат. Это гарантия by construction:
  рендер результата read-only и всегда идёт из последнего Core trace, а не
  из локального состояния выбора пользователя.
  - При первом запуске (`preferred_route_hint` ещё не установлен, значение
    `nil`/отсутствует) UI не передаёт hint в `prepare()` вовсе (Core
    трактует отсутствие hint как «нет предпочтения», не как «local»/«cloud»
    по умолчанию); degraded mode и partial fallback критерии, зависящие от
    `preferred_route_hint`, автоматически не срабатывают, пока hint не
    задан явно пользователем.
- **Degraded mode** — точный критерий активации:
  ```
  preferred_route_hint != nil AND
  selected_route == local AND
  selected_route != preferred_route_hint AND
  terminal_status ∉ truthful_refusal-family AND
  privacy_label == non-sensitive
  ```
  (`privacy_label` — Core-owned значение из trace, UI только ветвится на нём;
  `terminal_status ∉ truthful_refusal-family` эквивалентно «ответ успешно
  доставлен»). Если `preferred_route_hint == nil`, degraded mode не
  активируется — нет предпочтения, значит нет отклонения от него. Визуальный
  маркер и поведение — см. «Визуальная спецификация» выше.
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

- **Пустой `candidates[]`** при успешном `selected_route`: допустимо
  (Core мог не передать полный список кандидатов для успешного пути) — UI
  рендерит normal/degraded состояние по `selected_route`, но partial
  fallback/policy-refusal шаблоны, зависящие от конкретного элемента
  `candidates[]` (`reject_reason` preferred route), не могут быть показаны —
  UI молча пропускает этот под-текст, не подставляет заглушку.
  Пустой `candidates[]` при `terminal_status ∈ truthful_refusal-family` (в
  частности `both_routes_unavailable`) — это malformed payload (см.
  «Malformed/missing data» выше): `both_routes_unavailable` по определению
  подразумевает непустой список отклонённых кандидатов.
- **`preferred_route_hint` не задан** (первый запуск) — см. пункт выше в
  «Поведение UI»: hint не передаётся, degraded/partial fallback не
  вычисляются.
- **Race: `terminal_status` пришёл одновременно с IPC-ошибкой транспорта** —
  транспортный уровень обрабатывается отдельно от парсинга payload: если
  соединение оборвалось до получения полного trace, это `core_unavailable`
  независимо от того, что успело прийти частично. Если полный valid trace
  получен и только после этого соединение изменило статус (например,
  разрыв после успешного ответа) — уже отрендеренный trace не откатывается;
  `core_unavailable` относится только к следующему запросу.

## Доступность (accessibility)

- Все индикаторы состояния (degraded, refusal-шаблоны, `unknown_state`,
  `core_unavailable`) имеют ARIA-роли/live-region (`role="status"` или
  `role="alert"` в зависимости от срочности), так что screen reader
  озвучивает смену состояния без необходимости фокуса.
- Текст в каждом состоянии соответствует минимальному контрасту WCAG 2.1 AA
  (4.5:1 для обычного текста), включая `⚠ Degraded` индикатор и baner
  `core_unavailable`.
- Route selector и retry-контрол на `core_unavailable` полностью управляемы с
  клавиатуры (tab-order, Enter/Space активация), без mouse-only путей.
- Diagnostics view (raw `trace_id`/коды) доступен через тот же
  keyboard-navigable путь, что и остальной UI — не требует mouse-only
  контекстного меню.

## Тестовая стратегия

- **Unit:** localization table — build-time проверка, что набор ключей
  покрывает 100% значений `reason_code`/`terminal_status`/`safe_next_action`/
  `health_state` для текущего `schema_version` (см. «Локализация»); unit-тест
  на `unknown_state` fallback при неизвестном значении и на malformed payload
  (отсутствующее обязательное поле, неверный тип) → `core_unavailable`-путь.
- **Integration (мок Core IPC):** три сценария транспорта — success
  (валидный trace), timeout (нет ответа в срок), malformed (частичный/
  повреждённый payload) — каждый проверяется на итоговое визуальное
  состояние.
- **Матрица degraded mode** (таблица тестовых комбинаций, обязательна к
  покрытию):

  | `preferred_route_hint` | `selected_route` | `terminal_status` | `privacy_label` | Ожидаемое состояние |
  | --- | --- | --- | --- | --- |
  | nil | local | success | non-sensitive | normal (не degraded — нет hint) |
  | cloud | local | success | non-sensitive | degraded |
  | cloud | cloud | success | non-sensitive | normal |
  | cloud | local | success | sensitive | partial fallback, не degraded |
  | cloud | local | `both_routes_unavailable` | non-sensitive | refusal (both_routes_unavailable), не degraded |
  | local | local | success | non-sensitive | normal (hint совпадает с выбором) |

- **E2E:** полный проход по пяти визуальным состояниям (normal, degraded,
  refusal × 3 подтипа, `unknown_state`, `core_unavailable`) на реальном
  (не мокнутом) IPC-канале в staging-конфигурации Core.

## Проверки

- три состояния отказа (`both_routes_unavailable`, policy-отказ конкретного
  route, partial fallback) различимы в UI и не сливаются в одно сообщение;
- preferred route hint при конфликте не меняет отображаемый `selected_route`
  — UI показывает Core-возвращённое решение, а reject_reason preferred route
  виден отдельно;
- UI показывает фактический, а не желаемый route после fallback;
- degraded mode активируется и гаснет строго по определённому условию выше
  (включая `preferred_route_hint == nil` → никогда не активируется),
  проверено на матрице из «Тестовой стратегии»;
- при недоступности Core IPC UI показывает `core_unavailable`, не пытается
  показать предыдущий route как актуальный;
- malformed/missing обязательное поле в trace payload обрабатывается как
  `core_unavailable`, а не частичный рендер;
- неизвестное/будущее значение enum от Core не роняет UI и не рендерится как
  raw-код — попадает в `unknown_state` fallback с `safe_next_action =
  contact_support` и логированием `unsupported_enum`;
- localization table покрывает все значения `reason_code`/`terminal_status`/
  `safe_next_action`/`health_state`, используемые 02.3 для текущего
  `schema_version` — проверено build-time;
- пустой `candidates[]` не ломает normal/degraded рендер и не подставляет
  заглушку вместо отсутствующего под-текста;
- accessibility: ARIA live-region на смену состояния, контраст AA, полная
  keyboard-навигация route selector и retry.

## Критерии готовности

- 02.3 зафиксировал `schema_version` и полный список значений перечислений;
  UI не начинает реализацию до этой фиксации (frozen constraint выше);
- UI показывает фактический результат routing, выведенный напрямую из Core
  trace (без собственной интерпретации/новых кодов);
- cloud outage оставляет usable local degraded mode при выполнении точного
  критерия degraded mode, если он настроен;
- preferred route hint не может изменить `selected_route` — механизм
  read-only рендера описан и покрыт тестом;
- три состояния отказа, `unknown_state` и degraded mode понятны без просмотра
  технического trace — localization table покрывает весь закрытый набор
  enum-значений из 02.3 и проверяется build-time на полноту;
- sensitive/offline refusal показывает безопасное следующее действие
  (`safe_next_action`) как есть из trace, без собственной эвристики;
- недоступность Core IPC и malformed trace payload обработаны как одно общее
  нетерминальное состояние, не как один из `terminal_status`;
- визуальная спецификация (пять состояний + route selector) и accessibility
  требования выполнены и покрыты E2E;
- тестовая стратегия (unit/integration/degraded-mode-матрица/E2E) реализована
  и проходит в CI.
