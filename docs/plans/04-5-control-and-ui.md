# Этап 04.5: Контроль, IPC и UI

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.2 — команды читают и удаляют транскрипты; этап 04.3 —
состояние и политика; этап 04.4 — статус движка.

Разблокирует: 04.6 (бейдж источника в очереди памяти), 04.7 (карточки
предложений).

## Что этап отдаёт наружу

Полную пользовательскую поверхность: команды, события, индикатор записи,
глобальная пауза, панель «Слух» и режим capability микрофона.

## Что уже есть в коде

`oneof command` занят по тег 106 включительно: 92–102 — receipt-команды,
103–105 — plan-review, 106 — `ResolveRoutingDecision` из плана 02. События идут
через `event_type` и `payload` (`EventEnvelope`, поле `bytes payload = 5`),
поэтому новых сообщений в `oneof event` не требуется. `tray.ts` уже
перехватывает закрытие окна, создаёт трей из `evohime-agent.ico` и ставит
`setToolTip('EvoHime')` — готовое место для индикатора. `OperationsPanel`
существует. **`SafetyPanel` не существует**: в renderer есть только
`PermissionModePicker` (общий режим ask/read_only/full), отдельной панели
per-capability нет, поэтому она создаётся этим этапом. `ViewId` в `App.tsx:41`
— `'chat' | 'overview' | 'reviews' | 'operations'`. `preload/index.ts` не
меняется: он проксирует любую `RendererCommand` одним generic `invoke`.

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
  `SaveAmbientPolicy`, скачивание движка), возвращают `error_code` из
  фиксированного набора: `LISTENER_UNAVAILABLE`, `DEVICE_CONFLICT`,
  `DEVICE_DISCONNECTED`, `PERMISSION_DENIED`, `POLICY_INVALID`,
  `ENGINE_NOT_READY`, `STORAGE_FAILED`, `CONFIRMATION_REQUIRED` и
  `INVALID_ARGUMENT`. `ipc_bridge.rs` транслирует эти коды как есть, renderer
  сопоставляет их с локализованной строкой в `ListeningPanel`; неизвестный код
  показывает generic «Ошибка слушателя» и не трактуется как успешное изменение
  состояния.
- События (через существующий `EventEnvelope.payload`, JSON-полезная
  нагрузка): `ambient.state` — `{ state, reason, active_device_id }`,
  публикуется при любом изменении `ListeningState` независимо от источника
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
  источник истины (persisted в `evohime-core`, не в Electron main); любая
  из трёх точек входа (`tray.ts`, `globalShortcut`, `ListeningPanel`)
  отправляет одну и ту же команду `SetAmbientListening`, ветка обновляет
  state и публикует `ambient.state` — только это событие меняет UI, точки
  входа не хранят локальную копию состояния и не обновляют себя напрямую.
  При открытии панели renderer сперва вызывает `GetAmbientStatus`, чтобы не
  зависеть от того, застало ли событие открытым окно.
- `src/shared/api.ts`: девять записей в `RENDERER_COMMANDS` (список строк
  вида `'ambient.setListening'`), `CommandPayloads`, `CommandResults`;
  `shell-bridge.ts`: девять `case` в `dispatch()`. `preload/index.ts` не
  меняется — generic `invoke<C extends RendererCommand>` покрывает весь
  список.
- `tray.ts`: иконка-вариант и заголовок «Ева слушает» / «Микрофон на паузе»,
  пункт паузы; `globalShortcut` `Ctrl+Alt+M`. Индикатор **fail-visible**: при
  неизвестном состоянии показывается «проверка состояния», а не «выключено» и
  не создаётся ложное утверждение, что микрофон активен. К
  «неизвестному состоянию» относятся: таймаут IPC-запроса `GetAmbientStatus`
  дольше 5 секунд, ответ листенера с ошибкой на этот запрос, и рантайм,
  ещё не загруженный при старте приложения — во всех трёх случаях
  показывается «Слушание (проверка состояния…)» с иконкой ⚠️, а не
  «выключено».
- Renderer: `ViewId` (`App.tsx:41`) получает пятое значение `'listening'`,
  новый `ListeningPanel.tsx` —
  крупная строка состояния с причиной, переключатель паузы и подсказка про
  хоткей, «забыть последние 5 минут» и «удалить всё» (обе необратимые
  операции требуют подтверждения в модальном диалоге перед выполнением —
  без него команда не отправляется), список эпизодов (время, длительность
  речи, число высказываний, состояние извлечения) с раскрытием текста **по
  явному клику**, редактор политики (тихие часы, чёрные списки, срок
  хранения), установка движка распознавания, список доступных микрофонов и
  переключение устройства без перезапуска приложения. Список микрофонов
  обновляется по событию Windows `WM_DEVICECHANGE` нативным хуком в main/listener
  и рассылается в панель как обновлённый снимок через
  `GetAmbientStatus`/`ambient.state`; скрытый renderer не получает доступ к
  микрофону. Если отключено активное устройство,
  состояние переходит в `DEVICE_DISCONNECTED` и панель предлагает выбрать
  другое. Причины состояния — фиксированный enum `ListeningReason`:
  `ACTIVE`, `PAUSED`, `DEVICE_CONFLICT`, `DEVICE_DISCONNECTED`,
  `SYSTEM_SLEEP`, `MODEL_DEGRADED`, `LISTENER_ERROR`, `ENGINE_NOT_READY`; у
  каждого — локализованная строка-причина в `ListeningPanel` и в тултипе
  индикатора шапки.
- Индикатор записи в шапке `App.tsx`, не скрываемый ни одной вкладкой.
- Системный индикатор активности микрофона использует Windows API; приложение
  не пытается скрыть или подменить системный privacy-индикатор. Для accessibility
  смена состояния сопровождается коротким звуковым сигналом с настройкой
  отключения и локализованными текстами. Все новые строки UI (панель, трей,
  причины состояния, коды ошибок) заводятся в существующей системе
  локализации теми же ключами, что и остальной UI — новых механизмов
  локализации этот этап не вводит.
- Новый `SafetyPanel.tsx` (создаётся этим этапом, сегодня его нет): режимы
  capability по отдельным `Permission`, включая `microphone_listen`, и строка
  «за последний час: N высказываний, M кандидатов, K предложений». Панель
  соседствует с существующим `PermissionModePicker` и не подменяет его: общий
  режим по-прежнему меняется там, но переключение в «Полный доступ» не трогает
  `microphone_listen` (инвариант из 04.1).
- `listener-runtime.ts` (Electron main): скачивание рантайма движка —
  контролируемый pinned URL, проверка Authenticode/SHA-256/ABI, атомарный
  staging, approval, прогресс, повторные попытки и сохранение предыдущей
  рабочей версии при ошибке. Публичный интерфейс модуля:
  `initialize(): Promise<void>` — проверяет уже установленную версию при
  старте; `download(url): Promise<void>` — качает во временный staging-
  каталог; `verify(path): Promise<boolean>` — Authenticode + SHA-256 + ABI-
  проверка перед активацией; `activate(): Promise<void>` — атомарно
  переключает symlink/каталог на новую версию, старая версия остаётся на
  диске до следующего успешного `activate()` и используется для отката,
  если новая версия не проходит smoke-check при первом запуске;
  `getStatus(): RuntimeStatus` (`idle|downloading|verifying|approved|
  failed`) и `getVersion(): string` — синхронное чтение текущего
  состояния; модуль эмиттит `progress` (0–100) и `error` — оба
  транслируются наружу через событие `ambient.engine`, отдельного IPC-
  канала для рантайма нет. Откат при ошибке автоматический: `activate()`
  не вызывается, пока `verify()` не вернул `true`, поэтому предыдущая
  рабочая версия остаётся активной и пользователю показывается
  `ambient.engine{status: failed}` с кодом ошибки.

## Файлы

Все пути renderer/main — от корня репозитория, префикс
`desktop/evohime-electron/`.

- изменить: `crates/desktop-ipc/proto/evohime.desktop.proto`,
  `crates/evohime-core/src/ipc_bridge.rs`, `crates/evohime-core/src/lib.rs`,
  `desktop/evohime-electron/src/main/ipc/generated/protocol.{js,d.ts}`
  (регенерация),
  `desktop/evohime-electron/src/shared/api.ts`,
  `desktop/evohime-electron/src/main/shell-bridge.ts`,
  `desktop/evohime-electron/src/main/tray.ts`,
  `desktop/evohime-electron/src/main/index.ts`,
  `desktop/evohime-electron/src/renderer/src/App.tsx`,
  `desktop/evohime-electron/src/renderer/src/styles.css`;
- создать: `desktop/evohime-electron/src/renderer/src/ListeningPanel.tsx`,
  `desktop/evohime-electron/src/renderer/src/SafetyPanel.tsx`,
  `desktop/evohime-electron/resources/evohime-agent-listening.ico`,
  `desktop/evohime-electron/src/main/listener-runtime.ts`.

## Проверки

- `npm run check:protocol` зелёный после регенерации;
- `npm run typecheck` и renderer-тесты покрывают новую панель;
- unit-тесты `ipc_bridge.rs` покрывают все девять команд, включая
  `error_code`-ветки (`LISTENER_UNAVAILABLE`, `DEVICE_CONFLICT`,
  `POLICY_INVALID`, `ENGINE_NOT_READY`);
- пауза из трея, из хоткея и из панели приводит к одному состоянию
  (интеграционный тест: команда из одной точки входа немедленно отражается
  в состоянии двух других через `ambient.state`, без локальной рассинхронизации);
- при потере связи с листенером (таймаут `GetAmbientStatus` > 5с, ошибка
  ответа, рантайм не загружен) индикатор показывает «проверка состояния» с
  предупреждением, а не «выключено»;
- текст высказывания не приходит в списке, только по явному запросу
  (`GetAmbientEpisode`);
- «забыть последние 5 минут» и «удалить всё» без подтверждения в модальном
  диалоге не отправляют команду;
- «забыть последние 5 минут» удаляет и обновляет список без перезапуска;
- отключение активного микрофона (`WM_DEVICECHANGE`) переводит состояние в
  `DEVICE_DISCONNECTED` и обновляет список устройств без перезапуска;
- откат `listener-runtime.ts` при неудачном `verify()`/`activate()` не
  затрагивает уже установленную рабочую версию.

## Критерии готовности

- пользователь всегда видит, слушают ли его, и может остановить это тремя
  способами;
- удаление транскриптов доступно из UI, необратимо и требует явного
  подтверждения в модальном диалоге перед выполнением;
- контракт протокола не сломан и сгенерированные файлы совпадают;
- каждая из девяти команд имеет специфицированные поля запроса/ответа и
  определённый набор кодов ошибок, отображаемых пользователю понятной
  локализованной строкой.
