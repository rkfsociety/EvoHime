# Этап 04.5: Контроль, IPC и UI

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.2 — команды читают и удаляют транскрипты; этап 04.3 —
состояние и политика; этап 04.4 — статус движка и модуль установки рантайма.

Разблокирует: 04.6 (бейдж источника в очереди памяти), 04.7 (карточки
предложений).

## Что этап отдаёт наружу

Полную пользовательскую поверхность: команды, события, индикатор записи,
глобальная пауза, панель «Слух» и режим capability микрофона.

## Что уже есть в коде

`oneof command` занят по тег 106 включительно (`evohime.desktop.proto:938–952`):
92–102 — receipt-команды, 103–105 — plan-review, 106 —
`ResolveRoutingDecision` из плана 02. События идут через `event_type` и
`payload` (`EventEnvelope`, `bytes payload = 5`, `:966`), поэтому новых
сообщений в `oneof event` не требуется. `tray.ts` уже перехватывает закрытие
окна, создаёт трей из `evohime-agent.ico` (`tray.ts:29`) и ставит
`setToolTip('EvoHime')` (`:61`) — готовое место для индикатора.
`OperationsPanel` существует. **`SafetyPanel` не существует**: в renderer есть
только `PermissionModePicker` (общий режим ask/read_only/full), отдельной панели
per-capability нет, поэтому она создаётся этим этапом. `ViewId` в `App.tsx:41`
— `'chat' | 'overview' | 'reviews' | 'operations'`. `preload/index.ts` не
меняется: он проксирует любую `RendererCommand` одним generic `invoke`
(`preload/index.ts:30`).

Сверено дополнительно, потому что три пункта этапа на этом стоят:

- **Системы локализации в проекте нет.** UI-строки — русские литералы в
  модуль-локальных константных картах (`STATE_LABELS` в `App.tsx:28` —
  типовой образец). Никакого i18n-слоя, ключей и словарей не существует;
- **renderer'у отказано во всех разрешениях безусловно**:
  `setPermissionRequestHandler` и `setPermissionCheckHandler` всегда возвращают
  отказ, `setDevicePermissionHandler(() => false)` (`security.ts:59–67`).
  Значит список микрофонов физически не может прийти из
  `navigator.mediaDevices` — только из main/листенера. Это не осторожность
  плана, а свойство кода;
- `check-production-bundle.mjs` запрещает `child_process` в **preload и
  renderer**, но не в main — вызов внешних утилит из main допустим;
- `globalShortcut` сегодня **не используется нигде**: третья точка входа —
  новый механизм, а не подключение существующего.

## Содержание

- Proto: additive команды 107–115 — `SetAmbientListening`, `GetAmbientStatus`,
  `ListAmbientEpisodes`, `GetAmbientEpisode`, `DeleteAmbientTranscripts`,
  `ForgetAmbientWindow`, `GetAmbientPolicy`, `SaveAmbientPolicy`,
  `ResolveAmbientProposal`. Теги не переиспользуются и не переставляются;
  вместе с ними в `evohime.desktop.proto` заводятся новые типы, на которые
  ссылаются поля ниже: enum `ListeningState`, `ListeningReason`,
  `ExtractionState` и сообщения `AmbientDevice`, `AmbientEpisodeSummary`,
  `Utterance`, `QuietHours`, `AmbientPolicy` (proto-`AmbientPolicy` — транспорт
  для типа из 04.1, а не второй источник истины);
  `npm run generate:protocol` выполняется в том же коммите, сгенерированные
  файлы руками не правятся. Поля запрос/ответ по каждой команде:
  - `SetAmbientListening { bool enabled = 1; bool paused = 2; string
    device_id = 3; }` →
    `SetAmbientListeningResult { ListeningState state = 1; string error_code =
    2; }`. Инвариант полей: `enabled=false` означает `Stopped`, `enabled=true,
    paused=true` — `PausedByUser`, `enabled=true, paused=false` — запуск или
    продолжение; `device_id` меняет устройство только после проверки
    capability и политики;
  - `GetAmbientStatus {}` → `AmbientStatus { ListeningState state = 1;
    ListeningReason reason = 2; string active_device_id = 3; string
    engine_version = 4; bool engine_ready = 5; repeated AmbientDevice devices
    = 6; }`, где `AmbientDevice` содержит только `device_id`, bounded display
    name, `is_default` и `is_active`;
  - `ListAmbientEpisodes { int64 since_ms = 1; int32 limit = 2; string
    cursor = 3; }` → `AmbientEpisodeList { repeated AmbientEpisodeSummary
    episodes = 1; string next_cursor = 2; }`, где
    `AmbientEpisodeSummary { string episode_id = 1; int64 started_at_ms = 2;
    int64 speech_duration_ms = 3; int32 utterance_count = 4;
    ExtractionState extraction_state = 5; }`;
  - `GetAmbientEpisode { string episode_id = 1; }` → `AmbientEpisodeDetail {
    string episode_id = 1; repeated Utterance utterances = 2; }` — текст
    отдаётся только здесь, никогда в списке;
  - `DeleteAmbientTranscripts { repeated string episode_ids = 1; bool
    all = 2; bool confirmed = 3; }` →
    `DeleteAmbientTranscriptsResult { int32 deleted_count = 1; }`. Для
    `all=true` и удаления списка Core требует `confirmed=true` (или штатный
    approval), поэтому модальное окно Electron не является единственной
    границей безопасности;
  - `ForgetAmbientWindow { int64 window_ms = 1; bool confirmed = 2; }` →
    `ForgetAmbientWindowResult { int32 deleted_count = 1; }`; Core отвергает
    неподтверждённое удаление;
  - `GetAmbientPolicy {}` → `AmbientPolicy { repeated QuietHours quiet_hours
    = 1; repeated string blocklist_patterns = 2; int32 retention_days = 3;
    }`;
  - `SaveAmbientPolicy { AmbientPolicy policy = 1; }` →
    `SaveAmbientPolicyResult { bool applied = 1; string error_code = 2; }`;
  - `ResolveAmbientProposal { string proposal_id = 1; bool accepted = 2; }`
    → `ResolveAmbientProposalResult { bool applied = 1; }`.
  Команды, способные завершиться ошибкой (`SetAmbientListening`,
  `SaveAmbientPolicy`, установка движка), возвращают `error_code` из
  фиксированного набора: `LISTENER_UNAVAILABLE`, `DEVICE_CONFLICT`,
  `DEVICE_DISCONNECTED`, `PERMISSION_DENIED`, `POLICY_INVALID`,
  `ENGINE_NOT_READY`, `STORAGE_FAILED`, `CONFIRMATION_REQUIRED` и
  `INVALID_ARGUMENT`. `ipc_bridge.rs` транслирует эти коды как есть, renderer
  сопоставляет их со строкой в `ListeningPanel`; неизвестный код показывает
  generic «Ошибка слушателя» и не трактуется как успешное изменение состояния.
- События (через существующий `EventEnvelope.payload`, JSON-полезная
  нагрузка). Важно: публикация идёт не «в эфир», а через `append_event` в
  durable-таблицу `events`, откуда клиенты тянут хвост (`push_journal_tail`);
  таблица append-only и retention не имеет. Поэтому ни одно ambient-событие не
  несёт текста или `text_hash`, а удаление транскриптов вычищает связанные
  `ambient.*`-строки журнала — правило и тесты в 04.2. Набор событий:
  `ambient.state` — `{ state, reason, active_device_id }`, публикуется при
  любом изменении `ListeningState` независимо от источника
  (трей/хоткей/панель/системный sleep); `ambient.engine` — `{ status
  (idle|downloading|verifying|approved|failed), version, progress_pct }`,
  публикуется при смене статуса рантайма; `ambient.transcript` — `{
  episode_id, started_at_ms, utterance_count, extraction_state }` (без поля
  `text` — текст только через `GetAmbientEpisode`), публикуется по
  завершении экстракции эпизода; `ambient.retention` — `{ deleted_count,
  trigger (manual|policy|forget_window) }`, публикуется после любого
  удаления. Renderer подписывается на все четыре в `ListeningPanel` и на
  `ambient.state`/`ambient.engine` дополнительно в `App.tsx` (индикатор) и
  `tray.ts` (иконка).
- `ipc_bridge.rs`: девять веток по образцу `IndexWorkspace` и соответствующие
  `CoreCommand`. `ipc_bridge.rs` хранит `ListeningState` как единственный
  источник истины (в `evohime-core`, не в Electron main; конкретно — в registry
  по образцу `RoutingApprovalRegistry`, поскольку общего `CoreState` в коде не
  существует); любая из трёх точек входа (`tray.ts`, `globalShortcut`,
  `ListeningPanel`) отправляет одну и ту же команду `SetAmbientListening`,
  ветка обновляет state и публикует `ambient.state` — только это событие меняет
  UI, точки входа не хранят локальную копию состояния и не обновляют себя
  напрямую. При открытии панели renderer сперва вызывает `GetAmbientStatus`,
  чтобы не зависеть от того, застало ли событие открытым окно.
- `src/shared/api.ts`: девять записей в `RENDERER_COMMANDS` (список строк
  вида `'ambient.setListening'`, `:188`), `CommandPayloads`, `CommandResults`;
  `shell-bridge.ts`: девять `case` в `dispatch()`. `preload/index.ts` не
  меняется — generic `invoke<C extends RendererCommand>` покрывает весь
  список.
- `tray.ts`: иконка-вариант и заголовок «Ева слушает» / «Микрофон на паузе»,
  пункт паузы. Хоткей `Ctrl+Alt+M` — **новый механизм**: `globalShortcut` в
  проекте ещё не применялся, поэтому этап явно отвечает за регистрацию при
  готовности приложения, снятие на `will-quit` и за случай, когда комбинация
  уже занята другим приложением: `register()` возвращает `false`, и тогда UI
  сообщает, что хоткей недоступен, а не делает вид, что третья точка входа
  работает. Индикатор **fail-visible**: при неизвестном состоянии показывается
  «проверка состояния», а не «выключено», и не создаётся ложное утверждение,
  что микрофон активен. К «неизвестному состоянию» относятся: таймаут
  IPC-запроса `GetAmbientStatus` дольше 5 секунд, ответ листенера с ошибкой на
  этот запрос, и рантайм, ещё не загруженный при старте приложения — во всех
  трёх случаях показывается «Слушание (проверка состояния…)» с иконкой ⚠️.
- Renderer: `ViewId` (`App.tsx:41`) получает пятое значение `'listening'`,
  новый `ListeningPanel.tsx` — крупная строка состояния с причиной,
  переключатель паузы и подсказка про хоткей, «забыть последние 5 минут» и
  «удалить всё» (обе необратимые операции требуют подтверждения в модальном
  диалоге перед выполнением — без него команда не отправляется), список
  эпизодов (время, длительность речи, число высказываний, состояние
  извлечения) с раскрытием текста **по явному клику**, редактор политики
  (тихие часы, чёрные списки, срок хранения), установка движка распознавания,
  список доступных микрофонов и переключение устройства без перезапуска
  приложения. Список микрофонов обновляется по событию Windows
  `WM_DEVICECHANGE` нативным хуком в main/listener и рассылается в панель как
  обновлённый снимок через `GetAmbientStatus`/`ambient.state`. Другого пути и
  нет: renderer'у отказано во всех разрешениях и в доступе к устройствам
  (`security.ts:59–67`), поэтому `navigator.mediaDevices` для него закрыт по
  построению. Если отключено активное устройство, состояние переходит в
  `DEVICE_DISCONNECTED` и панель предлагает выбрать другое. Причины состояния —
  фиксированный enum `ListeningReason`: `ACTIVE`, `PAUSED`, `DEVICE_CONFLICT`,
  `DEVICE_DISCONNECTED`, `SYSTEM_SLEEP`, `MODEL_DEGRADED`, `LISTENER_ERROR`,
  `ENGINE_NOT_READY`; у каждого — строка-причина в `ListeningPanel` и в тултипе
  индикатора шапки.
- Индикатор записи в шапке `App.tsx`, не скрываемый ни одной вкладкой.
- Системный индикатор активности микрофона использует Windows API; приложение
  не пытается скрыть или подменить системный privacy-индикатор. Для
  accessibility смена состояния сопровождается коротким звуковым сигналом с
  настройкой отключения. Все новые строки UI (панель, трей, причины состояния,
  коды ошибок) — русские литералы в модуль-локальных константных картах, ровно
  как `STATE_LABELS` в `App.tsx:28`. Системы локализации в проекте нет, и этот
  этап её не вводит; формулировка «завести ключи в существующей системе
  локализации» из ранней редакции описывала механизм, которого не существует.
- Новый `SafetyPanel.tsx` (создаётся этим этапом, сегодня его нет): режимы
  capability по отдельным `Permission`, включая `microphone_listen`, и строка
  «за последний час: N высказываний, M кандидатов, K предложений». Панель
  соседствует с существующим `PermissionModePicker` и не подменяет его: общий
  режим по-прежнему меняется там, но переключение общего режима не трогает
  `microphone_listen` (инвариант из 04.1).
- Установка рантайма движка: модуль
  `src/main/update/listener-runtime.ts` создаётся в **04.4** вместе с его
  моделью доверия (SHA-256 из манифеста релизного канала; Authenticode — только
  там, где подпись реально есть, потому что собственные бинари проект не
  подписывает). Этот этап добавляет к нему только пользовательскую поверхность:
  вызов `initialize()` при старте, команду установки/обновления из
  `ListeningPanel`, трансляцию `progress`/`error` наружу одним событием
  `ambient.engine` (отдельного IPC-канала для рантайма нет) и отображение
  статусов `idle|downloading|verifying|approved|failed`. Дублировать здесь
  описание проверки и отката не нужно — оно принадлежит 04.4.

## Файлы

Все пути renderer/main — от корня репозитория, префикс
`desktop/evohime-electron/`.

- изменить: `crates/desktop-ipc/proto/evohime.desktop.proto`,
  `crates/evohime-core/src/ipc_bridge.rs`, `crates/evohime-core/src/lib.rs`,
  `desktop/evohime-electron/src/main/ipc/generated/protocol.{js,d.ts}`
  (регенерация через `npm run generate:protocol`),
  `desktop/evohime-electron/src/shared/api.ts`,
  `desktop/evohime-electron/src/main/shell-bridge.ts`,
  `desktop/evohime-electron/src/main/tray.ts`,
  `desktop/evohime-electron/src/main/index.ts` (регистрация и снятие
  `globalShortcut`),
  `desktop/evohime-electron/src/renderer/src/App.tsx`,
  `desktop/evohime-electron/src/renderer/src/styles.css`;
- создать: `desktop/evohime-electron/src/renderer/src/ListeningPanel.tsx`,
  `desktop/evohime-electron/src/renderer/src/SafetyPanel.tsx`,
  `desktop/evohime-electron/resources/evohime-agent-listening.ico` (каталог
  `resources` целиком уезжает в пакет через `extraResources`, отдельной правки
  `electron-builder.yml` не требуется).

`src/main/update/listener-runtime.ts` в этом списке нет намеренно: файл
принадлежит 04.4.

## Проверки

- `npm run check:protocol` зелёный после регенерации;
- ни одно `ambient.*`-событие не содержит `text`/`text_hash`, а после удаления
  эпизода его `ambient.*`-строки исчезают из журнала (тест в паре с 04.2);
- `npm run typecheck`, `npm test` и `npm run check:bundle` зелёные с новой
  панелью и новым `SafetyPanel`;
- unit-тесты `ipc_bridge.rs` покрывают все девять команд, включая
  `error_code`-ветки (`LISTENER_UNAVAILABLE`, `DEVICE_CONFLICT`,
  `POLICY_INVALID`, `ENGINE_NOT_READY`);
- пауза из трея, из хоткея и из панели приводит к одному состоянию
  (интеграционный тест: команда из одной точки входа немедленно отражается
  в состоянии двух других через `ambient.state`, без локальной рассинхронизации);
- занятая комбинация `Ctrl+Alt+M` (`globalShortcut.register` вернул `false`)
  показывается пользователю как недоступный хоткей, а не молча отключает
  третью точку входа;
- при потере связи с листенером (таймаут `GetAmbientStatus` > 5с, ошибка
  ответа, рантайм не загружен) индикатор показывает «проверка состояния» с
  предупреждением, а не «выключено»;
- текст высказывания не приходит в списке, только по явному запросу
  (`GetAmbientEpisode`);
- «забыть последние 5 минут» и «удалить всё» без подтверждения в модальном
  диалоге не отправляют команду, а Core отвергает такую команду и при обходе
  UI (`confirmed=false`);
- «забыть последние 5 минут» удаляет и обновляет список без перезапуска;
- отключение активного микрофона (`WM_DEVICECHANGE`) переводит состояние в
  `DEVICE_DISCONNECTED` и обновляет список устройств без перезапуска.

## Критерии готовности

- пользователь всегда видит, слушают ли его, и может остановить это тремя
  способами — либо честно узнаёт, что одна из трёх недоступна;
- удаление транскриптов доступно из UI, необратимо и требует явного
  подтверждения в модальном диалоге перед выполнением;
- контракт протокола не сломан и сгенерированные файлы совпадают;
- каждая из девяти команд имеет специфицированные поля запроса/ответа и
  определённый набор кодов ошибок, отображаемых пользователю понятной строкой.
