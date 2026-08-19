# Этап 02.4: UI

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 02.3 — UI строго потребляет trace schema, определённую там
(`schema_version`, `selected_route`, `reason_code`, `terminal_status`,
`safe_next_action`, `candidates[]` с `health_state`/`reject_reason`,
`fallback_count`, `privacy_label`). UI не вводит собственных кодов и не
переопределяет то, что уже зафиксировано в 02.3.

Это последний этап плана.

## Что этап отдаёт наружу

Честное отображение фактического route и причин отказа, целиком выводимое из
Core trace — без интерпретации или домысливания на стороне renderer.

## Контракт с Core (вход)

UI получает только то, что Core отдаёт через IPC как decision/trace (см.
«Формат trace и наблюдаемость» в [02-3](02-3-routing-and-budget.md)):

- `selected_route`, `terminal_status`, `reason_code`, `safe_next_action`;
- `candidates[]`: `route_id`, `health_state` ∈ `{healthy, degraded,
  unavailable}`, `reject_reason` (почему конкретный candidate не выбран —
  включая preferred route пользователя, если он был отклонён);
- `fallback_count`, `privacy_label` (anonymized, Core-owned определение
  sensitive/non-sensitive — UI не вычисляет это само);
- `run_id`/`trace_id` только для diagnostics view, не для основного UX.

`reason_code`, `terminal_status`, `safe_next_action` и `health_state` —
закрытые перечисления, зафиксированные в 02.3. UI хранит **таблицу
локализации** каждого значения в человекочитаемый текст; сырые коды никогда
не показываются пользователю напрямую (доступны только в debug/diagnostics
view). Если IPC возвращает значение вне известного перечисления — UI
показывает generic `unknown_state` с safe_next_action `contact_support`, а не
падает и не догадывается о смысле.

## Поведение UI

- Показывать фактически выбранный `selected_route`, а не желаемый.
- Три состояния отказа различаются напрямую по `terminal_status` (не
  придумываются UI): `both_routes_unavailable` (ни cloud, ни local),
  `policy_violation`/`route_unavailable`-подобные коды из candidate
  `reject_reason` (политический отказ конкретного route), и частичные fallback
  case, где `selected_route` есть, но отличается от preferred. Каждому
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
- **Degraded mode** — точный критерий активации:
  `selected_route == local AND selected_route != preferred_route_hint AND
  terminal_status ∉ {truthful_refusal-family} AND privacy_label ==
  non-sensitive` (последнее — Core-owned значение из trace, UI только
  ветвится на нём). Визуальный маркер: индикатор `⚠ Degraded` в заголовке
  ответа + краткая причина (`cloud unavailable`/`health degraded`, взято из
  `reason_code`). Отличие от обычного молчаливого fallback: degraded mode
  всегда видим и не сворачивается без явного действия пользователя.
- **Недоступность Core IPC** — отдельное, нетерминальное UI-состояние (не
  `terminal_status`, а транспортная ошибка): UI показывает
  `core_unavailable`, блокирует отправку задач, требующих routing decision, и
  предлагает retry. Не показывает route selector как активный и не
  подставляет предыдущий известный route.
- Локализация: таблица кодов → текст поддерживает как минимум текущий язык
  интерфейса; текст не содержит внутренних имён кодов (например,
  `policy_violation` → «Правила безопасности не позволяют использовать этот
  маршрут»).

## Проверки

- три состояния отказа (`both_routes_unavailable`, policy-отказ конкретного
  route, partial fallback) различимы в UI и не сливаются в одно сообщение;
- preferred route hint при конфликте не меняет отображаемый `selected_route`
  — UI показывает Core-возвращённое решение, а reject_reason preferred route
  виден отдельно;
- UI показывает фактический, а не желаемый route после fallback;
- degraded mode активируется и гаснет строго по определённому условию выше,
  проверено на матрице (`selected_route`, preferred, `terminal_status`,
  `privacy_label`);
- при недоступности Core IPC UI показывает `core_unavailable`, не пытается
  показать предыдущий route как актуальный;
- неизвестное/будущее значение enum от Core не роняет UI и не рендерится как
  raw-код — попадает в `unknown_state` fallback;
- localization table покрывает все значения `reason_code`/`terminal_status`/
  `safe_next_action`, используемые 02.3.

## Критерии готовности

- UI показывает фактический результат routing, выведенный напрямую из Core
  trace (без собственной интерпретации/новых кодов);
- cloud outage оставляет usable local degraded mode при выполнении точного
  критерия degraded mode, если он настроен;
- preferred route hint не может изменить `selected_route` — механизм
  read-only рендера описан и покрыт тестом;
- три состояния отказа и degraded mode понятны без просмотра технического
  trace — localization table покрывает весь закрытый набор enum-значений из
  02.3;
- sensitive/offline refusal показывает безопасное следующее действие
  (`safe_next_action`) как есть из trace, без собственной эвристики;
- недоступность Core IPC обработана как отдельное состояние, не как один из
  `terminal_status`.
