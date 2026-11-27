# Улей (Hive / EvaPulse)

Статус: актуальное описание реализации по аудиту checkout. Обновлено: 2026-11-27.
Связанные доменные ограничения и контракты: [`architecture/hive.md`](architecture/hive.md).

## Обзор

Улей — независимый от Core контур координации Королевы Евы, рабочих агентов,
сотовой симуляции (`HoneycombEngine`) и фонового наблюдения за репозиториями в
отдельном процессе `evohime-hive-daemon` (`evohime-hive`). `evohime-core` и
`evohime-hive` разделены на уровне крейтов и связаны только через минимальный
контракт `evohime-hive-contract`. Core сохраняет выполнение задач, лимиты,
approvals и маршрутизацию, `evohime-hive-daemon` обслуживает наблюдение,
`HiveStateSnapshot`, `HiveCityMapSnapshot` и сотовую карту через собственный
control pipe (`hive-daemon-client.ts`), а Electron показывает read-only проекцию и
отдельный системный чат `hive-system-chat`. Обычный чат может использовать те же
workflow-шаблоны, но его задачи не становятся агентами Улья.

`EvaPulseDaemon` владеет расписанием, статусом, cooldown и outbox;
`HiveSignalReceiver` валидирует сигнал и единственный создаёт Hive-задачу через
обычный `TaskCoordinator`. Перезапуск Pulse не переигрывает задачи Улья, а
перезапуск Улья не запускает Pulse заново.

## HiveSettings

`HiveSettings` хранится в Core в scope `hive` существующей таблицы
`schema_agent_configurations`. Неизвестные поля отклоняются (`deny_unknown_fields`),
настройки валидируются до использования.

| Поле | Назначение и ограничения |
| --- | --- |
| `schema_version` | Версия контракта настроек. Должна совпадать с текущей версией схемы. |
| `max_subagents_per_worker` | Жёсткий потолок подагентов одного рабочего; `0` означает отсутствие отдельного потолка, но действует workflow guard. |
| `max_parallel_subagents` | Максимум одновременно выполняющихся подагентов рабочего; `0` — без отдельного потолка. |
| `max_total_subagents_per_workflow` | Обязательный положительный общий потолок workflow. Значения по умолчанию: `16 / 4 / 64` для трёх лимитов. |
| `pulse_interval_secs` | Интервал пробуждения EvaPulse; по умолчанию 1800 секунд, значение ограничено допустимым диапазоном. |
| `daily_budget_usd` | Суточный потолок расходов фоновой работы. `0` запрещает запуск фоновых задач; бюджет проверяет Core, а не UI. По умолчанию `$0.50`. |
| `eva_workspace_path` | Отдельный существующий Git checkout EvoHime для self-development Королевы. Не может совпадать с текущим workspace или установленным бинарником. |
| `watched_repositories` | Список внешних существующих Git checkout, которые сканируются для proactive evolution. `eva_workspace_path` исключается из внешнего списка, но сканируется первым. |
| `hive_provider_profile` | Изолированный профиль основного провайдера Улья, например `literouter` или `rout`. |
| `hive_model` | Необязательная модель основного маршрута Улья. |
| `hive_fallback_provider_profile` | Изолированный профиль резервного провайдера. |
| `hive_fallback_model` | Необязательная модель резервного маршрута. |
| `hive_provider_limits` | RPM/RPH/RPD request caps основного маршрута; `0` означает отсутствие соответствующего cap. |
| `hive_daily_token_quota` | Суточная квота токенов основного маршрута; `0` — без квоты. |
| `hive_quota_reset_utc_midnight` | Переключает окно квоты основного маршрута на сброс в полночь UTC. |
| `hive_fallback_provider_limits` | RPM/RPH/RPD caps резервного маршрута. |
| `hive_fallback_daily_token_quota` | Суточная квота токенов резервного маршрута; `0` — без квоты. |
| `hive_fallback_quota_reset_utc_midnight` | Сброс квоты резервного маршрута в полночь UTC, если включён. |
| `hive_token_quota_configured` | Флаг, что основная квота была явно задана пользователем; отличает явную настройку от значения по умолчанию. |
| `hive_fallback_token_quota_configured` | Аналогичный флаг для резервной квоты. |
| `proactive_evolution_enabled` | Разрешает выбирать незавершённые roadmap-задачи в idle-цикле. По умолчанию выключен. |

API-ключи не входят в `HiveSettings` и не сериализуются в Core snapshot. Main
процесс Electron принимает их write-only, шифрует через Electron `safeStorage`
(на Windows — DPAPI) и хранит в `shell/provider.json`; renderer получает только
признак `configured`. Supervisor передаёт расшифрованные значения Core через
окружение на время запуска: `HIVE_API_KEY` и, если настроен, `HIVE_FALLBACK_API_KEY`.

## EvaPulse

`EvaPulseDaemon` запускается один раз при bootstrap Core. На каждом пробуждении он
читает bounded journal observations: окна dispatch, usage/стоимость, provider
caps, последние child-workflow и новые падения. Порядок `decide` — safety contract:

1. явное окно rate limit/429 — `Sleep` до `wake_at_ms`;
2. исчерпан `daily_budget_usd` — `Sleep` до следующих UTC-суток;
3. исчерпана quota/cap выбранного провайдера — `Sleep` до сброса или окна;
4. есть новые упавшие worker-задачи — `Recover`;
5. включён proactive evolution и найдены roadmap items — `Evolve`;
6. иначе — `AllGood`, с записью `EvaPulse: all good` в `core.jsonl`.

Для `Recover` или `Evolve` формируется bounded `EvaPulseSignal` без raw prompt,
ответа модели и секретов. После проверки TTL, identity и idempotency его принимает
`HiveSignalReceiver`, который создаёт задачу обычным coordinator path. Повторный
сигнал не создаёт вторую задачу.

### Каскад маршрутов и 429

Изолированный каскад имеет форму `hive` (primary LiteRouter или выбранный
профиль) → `hive_fallback` (только явно настроенный fallback) → `Sleep`. Переход
на глобальный маршрут обычного чата запрещён. Если primary исчерпал допустимые
модели/окно и fallback доступен, gateway может использовать только fallback;
если fallback отсутствует или его quota/cap тоже исчерпаны, Pulse спит.

429 обрабатывается отдельно от обычной ошибки: gateway публикует process-local
cooldown в `rate_limit_signal`, Pulse не повторяет запрос немедленно и не пытается
обойти 429 через fallback. Он ждёт конец окна и только затем повторно оценивает
состояние. Ошибка провайдера сама по себе не означает cooldown, пока не
зафиксирован rate-limit сигнал.

## Изолированный провайдер

При наличии `HIVE_API_KEY` конфигурация gateway создаёт маршрут с точным именем
`"hive"`, если такой маршрут ещё не задан. Профиль, base URL и модель берутся из
`HIVE_PROVIDER_PROFILE`, `HIVE_BASE_URL`, `HIVE_MODEL` или из Core-настроек
`hive_provider_profile`/`hive_model`. Аналогично `HIVE_FALLBACK_API_KEY` создаёт
`"hive_fallback"` с настройками `HIVE_FALLBACK_*`.

На IPC-границе клиент не может сам выбрать `hive`: Core назначает его только
подтверждённой shell-сессии Electron с `conversation_id = "hive-system-chat"`.
Значение `hive` из обычного чата, CLI или listener сбрасывается. Фоновые задачи
EvaPulse назначают маршрут внутри Core. Секреты не попадают в renderer, journal,
Hive snapshot или логи; при недоступном `safeStorage` ключ не записывается.

## `watched_repositories` и proactive evolution

EvaPulse открывает каждый валидный checkout через `EvaWorkspace`. Сканирование
идёт в порядке: `docs/development-plan.md`, затем отсортированные
`docs/plans/*.md`, затем корневые `TODO.md` и `ROADMAP.md`, затем
`docs/current-state.md`. Каталоги `.git`, `target` и `node_modules` не обходятся;
пути канонизируются и не могут выйти за пределы workspace.

Из файлов извлекаются только незавершённые Markdown-checkbox задачи: `- [ ]`,
`* [ ]`, `[ ]` и нумерованные варианты вроде `1. [ ]`. Уже обработанные элементы
идентифицируются по workspace, относительному пути и тексту. За один цикл берётся
ограниченное число кандидатов; пока предыдущая evolution-задача не завершена,
новые кандидаты не выдаются. Саморазвитие возможно только при включённом
`proactive_evolution_enabled`, валидном `eva_workspace_path` и непустом списке
кандидатов; внешние репозитории наблюдаются, но self-development workspace для
изменений остаётся отдельным.

## UI Electron

`HiveDashboard` в `OperationsPanel.tsx` — read-only консоль: она запрашивает
`HiveStateSnapshot`, показывает Королеву, департаменты, активных агентов, task
flow, метрики, бюджет, логи, approvals и настройки workspace/провайдера. Snapshot
обновляется оболочкой, а решения и approvals всё равно проходят Core IPC.
`HiveGraph` рисует Королеву в центре и bounded active-agent nodes вокруг неё;
цвет/состояние Королевы берётся из Core snapshot, а не вычисляется из обычного
чата.

Выделенный диалог имеет константу `HIVE_SYSTEM_CHAT_ID = "hive-system-chat"`.
Для него shell принудительно использует маршрут `hive`, отдельную conversation
history и typed event subscription. Мини-чат Улья читает только этот поток, поэтому
активность обычного чата не зажигает индикатор Королевы.

Индикатор «Королева печатает» (`queen-typing-indicator`) выводится из typed
событий системного чата, pending tasks, отправки сообщения и active approvals.
Это UI-состояние независимо от lifecycle EvaPulse: Pulse может спать, пока
Королева отвечает, и наоборот.

## Известные ограничения и баги

- Пустой реестр ролей или отсутствие child-workflow даёт департаменты `0 / 0`.
  Это производная пустая проекция, а не доказательство, что Core сломан.
- При ошибке чтения истории метрики snapshot исторически могут выглядеть как
  нули; новые budget-проекции используют `history_available = false` и должны
  показывать прочерк, а не выдуманный нулевой расход.
- Возможен зависший индикатор typing, если terminal event потерян или не дошёл до
  renderer. Поток должен гасить ожидание по terminal/accept failure, но этот
  boundary остаётся зоной наблюдения при reconnect и replay.
- UI пока не выражает достаточно явно разницу между `zero` (реально нулевое
  значение), `idle` (ничего не выполняется) и `data unavailable` (нет достоверных
  данных). Архитектору нельзя трактовать любой ноль как отсутствие активности.
- Lifecycle EvaPulse не входит в Hive snapshot: для `sleeping`, `unavailable`,
  причины cooldown и `wake_at_ms` нужно читать отдельную projection
  `eva_pulse.status` через `core.getEvaPulseStatus`.
