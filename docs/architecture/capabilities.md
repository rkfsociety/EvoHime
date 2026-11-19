# EvoHime — доменный контракт: инструменты, адаптеры, мосты и сервисы Core

Канонический контракт домена инструментов, адаптеров, мостов, CLI, голоса, зрения, git-публикации и сервисного графа. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).

## Ambient listening

Контракт постоянного слушания живёт в `crates/evohime-listener-contract` и не имеет побочных эффектов: ни файловой системы, ни часов, ни процессов. Это канонический источник состояний, лимитов, схемы политики, потолка проактивности и правил логирования; остальные части плана 04 (хранилище, процесс листенера, движок распознавания, UI, мост в память, проактивность) обязаны ссылаться на него, а не заводить второй набор значений. Транспортные копии типов в `evohime.desktop.proto` — именно транспорт, а не второй источник истины.

- **Capability.** `Permission::MicrophoneListen` (serde `microphone_listen`) по умолчанию `Deny`, и этот дефолт прописан в карте `PermissionEngine::new()` явно: `mode()` для отсутствующего ключа возвращает `Ask`, поэтому «просто не добавить вариант» означало бы «спрашивать». `set_all_modes` перечисляет остальные разрешения поимённо и микрофона не касается — Electron переотправляет сохранённый режим воркспейса при каждом открытии, а ветка `PermissionMode` в `ipc_bridge.rs` вызывает `set_all_modes` для любого значения, так что без исключения общий режим молча открывал бы микрофон. Глобальные режимы Core не персистит, поэтому единственный долговечный канал запрета — `permissions.json`; правило `{"permission": "microphone_listen", "pattern": "*", "mode": "deny"}` входит в `permissions.json.example`. Capability проверяет Core перед выдачей разрешения листенеру, а не только UI.
- **Состояния.** `ListeningState`: `Stopped`, `Starting`, `Listening`, `PausedByUser`, `PausedByPolicy`, `DeviceConflict`, `DeviceDisconnected`, `EngineUnavailable`, `Denied`. Поток захвата открыт только в `Listening`; пауза, тихие часы и чёрный список закрывают поток, а не фильтруют кадры. Переходы заданы таблицей: самопереходов нет (повтор состояния не является изменением и не публикует `ambient.state`), в `Listening` можно попасть только через `Starting`, в `PausedByUser` — в том числе прямо из `Stopped` и `Starting` (пользователь может включить слушание уже на паузе, и показывать ему «выключено» в этом случае было бы неправдой; микрофон при этом всё равно закрыт, потому что `is_capturing` истинно только для `Listening`), в `Denied` — из любого состояния, а из `Denied` — только в `Stopped`, поэтому отозванный микрофон не возобновляет работу без полного пути старта с проверкой capability. `DeviceConflict`, `DeviceDisconnected` и `EngineUnavailable` помечены как degraded: UI показывает «проверка состояния» и не утверждает, что слушание выключено.
- **Лимиты.** `AmbientLimits`: кадр 30 мс, pre-roll 300 мс, hangover 700 мс, минимум высказывания 400 мс, потолок высказывания 20 с, эпизод 10 минут, окно дедупликации 60 с. Snapshot неизменяем по образцу `run_policy`: renderer его показывает, но не поднимает.
- **Политика.** `AmbientPolicy` v1 — пауза, тихие часы, чёрные списки процессов и заголовков окон, retention. В тихие часы поток захвата закрыт полностью: ничего не распознаётся и не сохраняется. Политика валидируется целиком до применения: не более 16 окон тишины и 64 шаблонов, шаблон — glob не длиннее 128 байт и не более 8 wildcard, метасимволы регулярных выражений отвергаются, retention от 1 до 90 дней. Невалидная политика отклоняется целиком и никогда не деградирует в «слушать всё».
- **Проактивность.** `ProactivityBudget` — неизменяемый снимок потолка: 3 предложения в час, 10 в сутки, не менее 10 минут между предложениями. Текущие счётчики в snapshot не входят: они живут в `AmbientProactivityRegistry` в Core по образцу `RoutingApprovalRegistry` и передаются в решение как значение. Часы, ушедшие назад, интервал не открывают.
- **Логирование.** `StructuredLogger::write` принимает произвольный JSON, поэтому allow-list на него навесить нельзя; ambient-путь пишет не в сырой логгер, а через типизированный фасад `AmbientLogEvent` с фиксированным набором полей и через `AmbientLogSink`. Свободного текста в фасаде нет по типам: идентификаторы — bounded-newtype с ограниченным набором символов, так что протащить фразу через поле `id` невозможно. Текст речи, хеш текста, имя процесса и заголовок окна не попадают в логи никогда: короткую фразу перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому. Имена записей: `ambient.state`, `ambient.engine`, `ambient.transcript`, `ambient.retention`, `ambient.proposal`, `ambient.error`.
- **Ошибки.** Закрытый набор кодов: `LISTENER_UNAVAILABLE`, `DEVICE_CONFLICT`, `DEVICE_DISCONNECTED`, `PERMISSION_DENIED`, `POLICY_INVALID`, `ENGINE_NOT_READY`, `STORAGE_FAILED`, `CONFIRMATION_REQUIRED`, `INVALID_ARGUMENT`. Неизвестный код показывается как generic-ошибка слушателя и не трактуется как успешная смена состояния.
- **Хранение транскриптов.** Схема v25 добавляет `ambient_episodes`, `ambient_utterances` и `ambient_tombstones`; операции над ними живут в `crates/evohime-local-storage/src/ambient_store.rs`, а часы, сроки и файл политики — в `crates/evohime-core/src/ambient.rs`. Колонок для аудио нет по конструкции: тест проверяет, что `PRAGMA table_info` ambient-таблиц не содержит ни одной BLOB-колонки, поэтому «PCM не пишется на диск» — свойство схемы, а не дисциплины кода. Говорящий в v1 всегда `unverified`, и стор отвергает любое другое значение. Дедупликация идёт по `text_hash` в окне лимитов 04.1; `text_hash` считает Core, он живёт только в таблице и не покидает хранилище — короткую фразу перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому.
- **Retention.** Текст транскриптов живёт `EVOHIME_AMBIENT_RETENTION_DAYS` суток (по умолчанию 7, потолок 90; мусор в переменной даёт дефолт, а не «вечно»), метаданные эпизода и tombstone — 30 суток. Purge выполняется при старте Core и раз в час: `spawn_ambient_retention` делает стартовый прогон **до** первого `sleep`, в отличие от `spawn_approval_gc` и `spawn_receipt_retention`, где `sleep` стоит перед работой. `CancellationToken` эти задачи сегодня не используют, и ambient не вводит отмену в одиночку.
- **Удаление.** Транзакция фиксирует metadata-only tombstone (`episode_id`, время, причина из закрытого набора, число высказываний — без текста и без хеша) до того, как исчезает первое высказывание, затем удаляет высказывания и эпизод, отклоняет производных memory-кандидатов с `provenance_source_id = episode_id` причиной `source_deleted` и вычищает ambient-строки журнала. `forget_window(minutes)` работает по замкнутому окну `[now - minutes, now]`: эпизод, лишь пересекающий границу, не удаляется целиком, у него пересчитываются счётчики, а опустевший уходит в той же транзакции. Кандидаты отклоняются у всех задетых эпизодов, а не только у удалённых: provenance ведёт к эпизоду, а не к высказыванию.
- **Ambient-строки в durable journal.** События публикуются через `append_event` в таблицу `events`, у которой нет retention вообще. Поэтому: в ambient-событии нет ни текста, ни `text_hash` (это гарантирует типизированный фасад 04.1); `task_id` ambient-события — это `episode_id`, а у событий без эпизода — `ambient-session`, чтобы удаление шло по существующему индексу `idx_events_task_sequence`, а не сканом BLOB-payload; удаление эпизода и `forget_window` удаляют соответствующие `ambient.%`-строки в той же транзакции; для всех `ambient.%`-строк действует собственный срок хранения в 30 суток, равный retention метаданных эпизода. Это первый в кодовой базе `DELETE` из `events` — до сих пор оттуда не удаляли ничего, даже при очистке истории ревью (`review.history_cleared` — маркерное событие, а не `DELETE`). Для читателей журнала это безопасно: курсор `push_journal_tail` монотонен по `sequence_id` и дырки переносит.
- **Остаточное окно в бэкапах.** Удаление ambient-эпизода вращает backup-контейнеры той же продовой константой, что и forget памяти (7 суток). Это чистит только состарившиеся контейнеры: в снимке моложе семи суток удалённый транскрипт физически остаётся на диске. Окно называется пользователю прямо в тексте про удаление, а не заметается под «удалено безвозвратно».
- **Файл политики.** `ambient-policy.json` в data dir пишется атомарно: временный файл, `sync_all`, owner-only ACL, `rename`, ACL ещё раз. Отсутствующий файл означает «ещё не настраивали» и читается как дефолт; повреждённый или невалидный — как дефолт **с включённой паузой**: fail-safe в пользу тишины. Невалидная политика не сохраняется вовсе, иначе следующий старт молча встал бы на паузу.
- **Ошибка записи.** Отказ SQLite не ретраится: Core возвращает `STORAGE_FAILED`, не создаёт ложную запись и best-effort публикует `ambient.error` с этим кодом (отдельного имени `ambient.storage_error` нет — набор имён записей закрыт контрактом 04.1). Листенер помечает высказывание как потерянное и продолжает со следующего сегмента.
- **Процесс listener (04.3).** `evohime-listener.exe` запускается supervisor в отдельном Windows Job Object с bounded memory/CPU и собственным restart budget. Job Core создаётся на каждое поколение Core, поэтому падение или рестарт Core не закрывает listener; завершение supervisor закрывает его job и не оставляет сироту. Listener подключается к отдельному owner-only pipe `<core-pipe>-listener`, а shell продолжает использовать основной pipe. Оба endpoint используют отдельную nonce/HMAC-аутентификацию роли `listener`.
- **Аудио и сегментация.** `evohime-listener-audio` открывает cpal/WASAPI shared input, держит PCM только в bounded ring buffer, best-effort применяет `VirtualLock`, ресемплирует 32/48 кГц в 16 кГц и выполняет RMS/zero-crossing VAD. Фикстурный сегментатор использует три voiced-кадра, pre-roll 300 мс, hangover 700 мс, минимум 400 мс и потолок 20 с; длинная речь получает `continued`. Крейт не содержит filesystem API.
- **Движок распознавания (04.4).** Локальный whisper.cpp загружается из проверенного каталога инструментов: `EVOHIME_LISTENER_TOOLS_DIR` → `EVOHIME_TOOLS_DIR\listener` → `%LOCALAPPDATA%\EvoHime\tools\listener`. Первый каталог с валидным манифестом побеждает; недоступный — переход к следующему кандидату, а не ошибка. Биндинги сделаны на `libloading` вручную, потому что сборка продукта не должна требовать CMake: self-update ставит только Git, Node, Rustup и MSVC Build Tools. Отсутствие DLL или модели даёт `EngineUnavailable` с кодом (`tools_dir_missing`, `manifest_missing`, `manifest_invalid`, `manifest_path_escapes`, `file_missing`, `size_mismatch`, `hash_mismatch`, `unexpected_file`, `signature_missing`, `signature_untrusted`, `abi_unsupported`, `load_failed`, `model_load_failed`), а не тишину и не панику.
- **Корень доверия к рантайму — хеш, подпись продукта не входит в текущий release scope.** Единственный локальный корень доверия — SHA-256 каждого файла из `listener-runtime.json`, полученного тем же релизным каналом GitHub, что и установщик продукта (по образцу `release-installer.ts`, с теми же потолками размера и таймаутом). Authenticode signing для собственного installer и shipped binaries явно отложен и не является release gate’ом текущего цикла. Для `onnxruntime.dll` сохраняется отдельная проверка подписи Microsoft: отсутствие или недоверенная подпись означают отказ; собственный `whisper.dll` остаётся unsigned штатно. Манифест приходит по сети, поэтому путь из него проверяется на выход за каталог, а любая необъявленная `*.dll` рядом с `whisper.dll` блокирует загрузку: иначе загрузчик Windows подтянул бы её как зависимость мимо проверки хеша. Раскладка `whisper_full_params` зеркалится в коде и сверяется с размерами из манифеста (`48`/`304`) до первого вызова; чужой ABI — `abi_unsupported`, а не попытка вызвать.
- **Поставка рантайма.** Скачивает только Electron main (`listener-runtime.ts`): TLS, потолки размера, таймаут, SHA-256, staging-каталог и атомарный `rename` манифеста последним шагом. Пока манифест не переименован, листенер видит прежний рабочий набор. Неудача даёт ограниченный backoff (15 с → 1 мин → 5 мин) и видимое сообщение; файлы прежних версий удаляются только после успешного переключения, а занятый (ещё отображённый в память листенера) файл остаётся на месте, а не роняет установку. Ни агент, ни Core в сеть за рантаймом не ходят, ничего не применяется незаметно: обновление предлагается пользователю на вкладке «Распознавание речи».
- **Дедупликация и бюджет.** Повтор подавляется в листенере, до отправки в Core: нормализация NFKC + нижний регистр + без пунктуации, точное совпадение в окне 60 с и near-dup по мере Сёренсена–Дайса на множествах слов ≥ 0.9 против пяти предыдущих высказываний. Подавленное считается счётчиком, а не пишется. RTF измеряется на каждом высказывании; пять подряд выше 0.5 переключают модель на следующую в лестнице `small → base → tiny`, каждая смена публикует `ambient.engine`. После `tiny` листенер переходит в `PausedByPolicy` с причиной `engine_degraded`. Обратного хода нет: улучшение нагрузки не возвращает тяжёлую модель в той же сессии, иначе на границе порога модель перезагружалась бы туда-сюда. Ступень, которой нет в поставке, — это сразу деградация, а не тихое «оставим как было». Хранилищная дедупликация по `text_hash` остаётся второй линией.
- **Политика и privacy.** Core передаёт listener валидированную политику; пауза, quiet hours и blocklist закрывают поток захвата, а не отбрасывают кадры. Foreground process/title проверяются раз в listener loop через Windows API. При отсутствии Core listener остаётся `PausedByPolicy`, а сброс `reset_buffers` очищает только ring/VAD/незавершённый сегмент. `scripts/ambient-privacy.tests.ps1` сканирует аудио-крейт на filesystem I/O и запускает детерминированные аудио-тесты.
- **Контроль, IPC и UI (04.5).** Десять additive-команд `oneof command` с тегами 107–116: `SetAmbientListening`, `GetAmbientStatus`, `ListAmbientEpisodes`, `GetAmbientEpisode`, `DeleteAmbientTranscripts`, `ForgetAmbientWindow`, `GetAmbientPolicy`, `SaveAmbientPolicy`, `ResolveAmbientProposal`, `ListAmbientProposals` (116, этап 04.7). Ответы уходят JSON-полезной нагрузкой существующего `EventEnvelope` под именами `ambient.listening`, `ambient.status`, `ambient.episodes`, `ambient.episode`, `ambient.deleted`, `ambient.forgotten`, `ambient.policy`, `ambient.policy_saved`, `ambient.proposal_resolved`, `ambient.proposals`; новых сообщений в `oneof event` не заводится. Имя ответа на решение — `ambient.proposal_resolved`, а не `ambient.proposal`: последнее занято durable-записью журнала, и подмена списка карточек ответом на команду сломала бы проекцию renderer'а. Инвариант полей `SetAmbientListening`: `enabled=false` — `Stopped`, `enabled=true, paused=true` — `PausedByUser`, `enabled=true, paused=false` — запуск или продолжение; `device_id` меняет устройство только после проверки bounded-контракта идентификаторов и наличия устройства в снимке.
- **Единственный источник истины о состоянии.** `AmbientListeningRegistry` в `crates/evohime-core/src/ambient.rs` — по образцу `RoutingApprovalRegistry`, потому что общего `CoreState` в проекте нет. Три точки входа — трей, глобальный хоткей `Ctrl+Alt+M` и вкладка «Слух» — отправляют одну и ту же команду и не хранят локальной копии состояния: их обновляет только событие `ambient.state`. Оптимистичное состояние ставится в реестре сразу после успешной отправки команды листенеру, но реальное всегда приходит от листенера и перекрывает его. Ambient-события пишутся прямо в журнал, поэтому после записи вызывается `TaskCoordinator::notify_journalled` — без этого сигнала `push_journal_tail` не разбудил бы открытое окно.
- **Fail-visible индикатор.** Стартовое состояние реестра — `EngineUnavailable`, а не `Stopped`: пока листенер не подключился, Core не знает, читается ли микрофон. Оболочка спрашивает `GetAmbientStatus` при старте и при открытии панели; молчание дольше пяти секунд, ошибка ответа и незагруженный рантайм дают «Слушание: проверка состояния…» с предупреждением. Утверждение «выключено» делается только по известному состоянию.
- **Три точки входа.** Хоткей регистрируется через `globalShortcut` при готовности приложения и снимается на `will-quit`. Занятая комбинация (`register` вернул `false`) объявляется недоступной отдельной командой оболочки `ambient.hotkeyStatus` — это единственная ambient-команда, которая не ходит в ядро, потому что ответ знает только main-процесс. Трей показывает вариант иконки `evohime-agent-listening.ico` и заголовок состояния, а пункт паузы выключен там, где микрофон и так закрыт.
- **Устройства.** Renderer'у отказано во всех разрешениях и в доступе к устройствам (`security.ts`), поэтому `navigator.mediaDevices` для него закрыт по построению, и единственный источник списка — процесс листенера. `evohime-listener-audio` перечисляет входы cpal и держит message-only окно с подпиской `RegisterDeviceNotificationW` на `KSCATEGORY_CAPTURE`; `WM_DEVICECHANGE` перечитывает список и отправляет его снимок в Core. Идентификатор устройства выводится из его имени и приводится к charset bounded-идентификаторов 04.1, поэтому через поле `device_id` нельзя протащить фразу. Отказ подписки уезжает в `watching=false`, и панель говорит, что список не обновится сам, — вместо того чтобы показывать снимок как живой. Смена устройства закрывает прежний поток и открывает новый без перезапуска процесса; пропажа выбранного устройства даёт `DeviceDisconnected`, а не тихий откат на умолчание.
- **Тихие часы в рантайме.** `PolicyUpdate` несёт окна тишины парой параллельных списков минут суток, `enabled` и выбранное устройство. Листенер пересматривает желаемое состояние каждые 20 секунд по локальным часам (`GetLocalTime`), поэтому наступившее окно тишины закрывает поток само, без команды снаружи.
- **Удаление из UI.** «Забыть последние 5 минут» и «удалить всё» требуют подтверждения в модальном диалоге, а Core независимо отвергает команду с `confirmed=false` кодом `CONFIRMATION_REQUIRED`: обход UI не даёт больше прав. Текст высказываний пересекает границу IPC только в ответе `GetAmbientEpisode`, который отправляется по явному клику «Показать текст»; список эпизодов несёт лишь время, длительность речи, число высказываний и состояние извлечения.
- **Границы эпизода.** Сообщения «эпизод кончился» в протоколе листенера нет — он присылает высказывания и флаг продолжения, — поэтому границу проводит Core: эпизод закрывается началом следующего, минутой тишины (проверка раз в 20 секунд) и разрывом связи с листенером. Закрытие и есть триггер ambient-извлечения: закрытый эпизод уходит в Core командой `ExtractAmbientMemory`, и `extraction_state` эпизода проходит `pending` → `done`/`failed`. Без этой границы извлечение не наступало бы никогда.
- **Ограниченная проактивность (04.7).** По услышанному Ева может произвести ровно два эффекта: карточку-предложение в очереди и неисполняемое напоминание. Список закрыт типом `ProactiveEffect` в `crates/evohime-core/src/ambient_proactivity.rs`, и `authorize_proactive` отказывает `StartTask`, `ToolCall`, `FileWrite` и `NetworkRequest` **до** любого эффекта, раньше mute и раньше бюджета. Это инвариант, а не настройка: новый проактивный эффект — правка перечисления и его негативных тестов, а не значение конфигурации.
- **Откуда берётся предложение.** 04.6 отбрасывает из речи `constraint` и `decision` до persistence именно потому, что они влияют на действия. 04.7 не воскрешает их как память: `decision` становится предложением задачи (`suggestion`), `constraint` — напоминанием (`reminder`), и оба ждут клика. Остальные виды остаются кандидатами в память и предложением не становятся: предпочтение или факт действия не требуют.
- **Два ключа, а не один.** Дедупликация идёт по `proposal_key` = вид + тема + округлённый до часа момент; он и стоит под `UNIQUE`. Постоянный mute («больше не предлагать такое») идёт по `mute_key` = вид + тема, **без времени**. Один ключ на обе роли не работает: со временем внутри ключа mute заглушил бы ровно одну временную корзину и молча перестал бы действовать через час, а без времени `UNIQUE` запретил бы любое повторное предложение по той же теме после истечения предыдущего. Тема в обоих ключах — bounded-токен `SubjectKey`: ASCII-слаг, а для кириллицы короткий отпечаток; пробелов в нём нет по построению, поэтому через это поле нельзя протащить фразу.
- **Потолок доказуем и не обходится очередью.** `ProactivityBudget` из 04.1 неизменяем: не больше 3 предложений в час и 10 в сутки, не чаще одного раз в 10 минут, плюс пауза и тихие часы общей политики. Текущие счётчики живут в `AmbientProactivityRegistry` (`ambient.rs`) и персистятся строкой таблицы v26, поэтому перезапуск Core не обнуляет часовой потолок. Превышение **отбрасывает** предложение со счётчиком в трассе, а не копит его: иначе после часа тишины пользователь получил бы десять карточек разом. Счётчик поднимается только после того, как карточка действительно появилась, поэтому дубликат бюджета не тратит.
- **Схема v26.** Additive-миграция поверх ambient-хранилища v25: `ambient_proposals` (уникальные `proposal_id` и `proposal_key`, `state` из пяти состояний автомата, `occurrences`, nullable `source_episode_id` с `ON DELETE SET NULL` и пара `source_deleted_at`/`source_deleted_reason` под `CHECK` «оба NULL либо оба заполнены»), `ambient_proposal_mutes` по `mute_key` и `ambient_proactivity_counters` — одна строка на ambient-профиль. Миграция транзакционна и получает обычный backup до изменения схемы.
- **Порядок при удалении источника задан явно.** Удаление эпизода переводит связанные предложения в `expired` с причиной `source_deleted`, и этот `UPDATE` выполняется **до** удаления строки эпизода, в той же транзакции с его tombstone. Наоборот нельзя: `ON DELETE SET NULL` сработал бы первым и обнулил связь, после чего найти затронутые предложения было бы уже нечем. FK при этом удаление источника не блокирует.
- **Событие `ambient.proposal`.** Пятое ambient-событие журнала. Payload несёт только `proposal_id`, `episode_id`, `kind`, bounded `subject_key` и `proposal_state`; текста карточки и темы человеческими словами в нём нет — прецедент прямой, `memory.pending` по той же причине не несёт `statement`. `task_id` строки — это `episode_id`, поэтому `ambient.proposal` уходит вместе с эпизодом по существующему индексу `idx_events_task_sequence`, как и `ambient.transcript`. Человекочитаемый текст renderer получает командой `ListAmbientProposals`, а хранится он в `ambient_proposals`, то есть под ambient-retention.
- **Жизнь карточки.** 24 часа без ответа — это ответ «нет»: `purge_ambient` и чтение списка переводят просроченное в `expired`, поэтому вчерашнее предложение не висит как ждущее. Терминальное состояние не переигрывается: второй клик отвечает «уже решено», а не меняет решение.
- **Принятие проходит штатный путь.** Решение несёт обязательный `idempotency_key` (по образцу `ConfirmMemory`): без него двойной клик по карточке породил бы две задачи, а повтор с тем же ключом возвращает первое решение. Принятое создаёт обычную запись `work_items` в проекте `ambient-proposals` со статусом `backlog` — то есть ничего не запускающую саму по себе; `source_ref` несёт `episode_id`, тот же провенанс, по которому удаление эпизода находит своих кандидатов памяти. У напоминания признак неисполняемости записан в данных полем `non_goals`, а не подразумевается. Отдельного ambient-receipt нет: запуск такой задачи идёт обычным путём Core со всеми его approval-гейтами.
### Голосовые команды и каталог приложений

По услышанному Ева умеет открыть приложение. Это не расширение проактивности 04.7, а отдельный путь: проактивность — это то, что Ева делает **без** просьбы, а здесь просьба произнесена вслух. Закрытый список эффектов 04.7 при этом не тронут — карточка и напоминание остаются единственным, что порождает сама Ева.

- **Обращение обязательно.** `crates/evohime-core/src/voice_command.rs` разбирает высказывание детерминированно, без модели: сначала имя (`ева`, `эва`), затем — не дальше двух служебных слов — глагол открытия, затем название. Фраза без обращения командой не является вообще: рядом с микрофоном разговаривают люди, и «открой окно» в их разговоре ничего не запускает. Обращение после глагола («открой хром, Ева») тоже не считается — иначе им становился бы любой пересказ. Цель ограничена четырьмя словами и 64 символами: название приложения — это не остаток разговора.
- **Каталог вместо пути.** Открыть можно только запись каталога `crates/tool-runtime/src/app_catalog.rs`; ни услышанная фраза, ни ответ модели не доходят до `CreateProcess` в виде пути. Источников три, по возрастанию доверия: встроенные системные приложения Windows (путь проверяется на существование), `App Paths` реестра (HKCU, HKLM и WOW6432Node) и пользовательский `app-catalog.json` в data dir. Найденное автоматически дополняет выверенную запись только синонимами: реестр не знает, что `Notepad` по-русски «Блокнот». Исполняемый файл MSIX-пакета подменяется его alias'ом в `%LOCALAPPDATA%\Microsoft\WindowsApps` — прямой запуск из `Program Files\WindowsApps` запрещён ACL. Каталог перечитывается не чаще раза в пять минут.
- **Догадок нет.** Совпадение ищется по точному синониму, затем по границе слова, затем по категории («браузер» — фиксированный приоритет, а не случайный выбор). Неизвестное название и название, подходящее сразу нескольким приложениям, дают одно и то же: не происходит ничего. Подставить за пользователя один вариант из трёх было бы хуже молчания.
- **Клик, а не микрофон.** По умолчанию услышанная команда становится карточкой в очереди `VoiceCommandRegistry` (в памяти, 5 минут жизни, не больше 8 штук, повтор заменяет карточку вместо второй). Открывает приложение клик. Автозапуск существует, но только как явный выбор в ambient-политике: поля `voice_commands` и `voice_commands_autorun`, по умолчанию «распознавать, но спрашивать». Пауза слушания и выключенные команды останавливают разбор целиком.
- **Событие и заголовок врозь.** Шестое ambient-событие журнала — `ambient.voice_command` с полями `command_id`, `kind`, `app_id`, `command_state`. Ни фразы, ни её обрывка в нём нет: `app_id` — ключ каталога, то есть выбор Core из заранее известного списка. Человекочитаемый заголовок renderer получает командой `ListVoiceCommands` — тем же способом, каким читается текст карточки предложения.
- **Две additive-команды.** `ListVoiceCommands` (127) и `ResolveVoiceCommand` (128); ответы — `ambient.voice_commands` и `ambient.voice_command_resolved`. Карточка снимается с очереди до запуска, поэтому двойной клик не открывает два окна: второй ответ — `not_found`. Голосовые поля `AmbientPolicy` объявлены `optional bool`: клиент, не знающий о них, их не шлёт, и Core подставляет сохранённое значение вместо того, чтобы выключить настройку молчанием.
- **Инструменты `app_open` и `app_list`.** Тот же каталог доступен агенту из чата. Вход `app_open` — название, а не путь; разрешение — `shell_execute`, риск — `Medium` (произвольной командной строки, в отличие от `shell_execute`, здесь нет), превью подтверждения — `app_open`. `app_list` только читает каталог и классифицирован как `None`.
- **Открытое приложение переживает Core.** Запуск идёт с `CREATE_BREAKAWAY_FROM_JOB`, а job object супервизора получил `JOB_OBJECT_LIMIT_BREAKAWAY_OK`. Это не ослабление `KILL_ON_JOB_CLOSE`: `SILENT_BREAKAWAY` не выставлен, поэтому отвязывается только процесс, созданный с явным флагом, а дерево самого Core по-прежнему умирает вместе с ним. Если отвязка запрещена, запуск повторяется без флага — приложение всё равно откроется.

- **Разрешения в UI.** `SafetyPanel` перечисляет разрешения по отдельности и переключает единственное, которое не подчиняется общему режиму, — `microphone_listen`; включение идёт той же командой `SetAmbientListening`, которая в ядре делает именованный вызов `set_mode(Permission::MicrophoneListen, …)`. Общий режим по-прежнему меняется в `PermissionModePicker`, и `set_all_modes` микрофон не трогает.

## Tool manifest, toolkit catalog и Action Console

Каждый builtin/toolkit tool описывается versioned `tool/manifest/v1` в
`crates/tool-runtime/src/manifest.rs`. Canonical hash манифеста прикрепляется
к model loadout; input schema для registry, model и recovery берётся из
единого `builtin_input_schema`, без таблицы схем в Core. `mcp_call` получает от
модели только `server_id`, `tool_name` и `params`; endpoint разрешается Core
`WorkflowRegistry` по allowlist сервера.

Builtin manifests материализуются один раз при регистрации инструмента и
переиспользуются registry-кэшем для loadout, preflight и IPC-выдачи; изменение
регистрации заменяет соответствующую запись кэша.

Установленные toolkit-версии хранятся в SQLite `toolkit_versions`, переходы и
rollback — в `toolkit_audit`. Rollback атомарно выключает прежнюю активную
версию и включает выбранную; quarantined/unavailable версии не становятся
исполняемыми.

Approval Console использует durable receipt intent, exact-call binding,
idempotency key, expiry и replay-safe решение. Electron отображает только
Core-события и передаёт grant/reject/cancel обратно через IPC.

Tool lifecycle telemetry записывается в EventJournal, экспортируется в
bounded redacted JSONL и проецируется в Operations Panel: calls, results и
approval requests. Детерминированные manifest/hash/policy evals находятся в
`crates/evohime-core/src/evals.rs`.

## Conversation Bridge Adapters v1 (план 94, реализован 2026-09-03)

`ConversationBridge` и `ThreadBinding` — Core-owned versioned metadata,
связывающие allowlisted provider/thread с local conversation. Bridge хранит
только principal identity и pairing hash (не token/credential); pairing,
revision fence и revoke проверяются Core против durable paired state.

Inbound messages принимаются только после binding/principal authorization и
deduplicate по `message_id` в bounded SQLite queue (256). Remote control
ограничен typed `Attention`, `ApprovalReply` и `HumanWorkItemReply`; он не
выдаёт capabilities, workspace paths, credentials или standing approval.
Outbound bridge projection — bounded redacted metadata с provenance ID, без
message text/raw prompt/tool output. Authenticated additive IPC command 243/
event 88 и schema 89 обслуживают Core handler; Electron показывает только
состояние и revoke/clear actions. Сетевые provider adapters и ingress
transport остаются отдельным следующим слоем, поэтому потеря bridge не
затрагивает локальную conversation.
## Agentic Browser Session v1 (план 55, реализован 2026-09-01)

`evohime-core::agentic_browser_session` владеет versioned lifecycle, session
scope, page revision/fingerprint, bounded snapshots и exclusive
`Agent/Human` control generation. Любая mutation проверяет exact session/page
revision; takeover и возврат control увеличивают revision и потому
инвалидируют старые refs. Durable SQLite schema v54 хранит только lifecycle
metadata, без DOM, cookies, credentials, CDP endpoint или host paths.

Authenticated IPC additive command 204/event 53 возвращает только bounded
metadata projection. Raw `EVOHIME_BROWSER_CDP_URL`, CSS selector и прямой
workspace screenshot path не являются production contract; legacy path получает
typed `legacy_disabled`, отсутствие packaged backend — `unavailable`. Browser
network policy обязана проверять каждый redirect и фактический resolved IP,
включая DNS rebinding, а default profile — `ephemeral_clean`.

Native manifest содержит `browser_backend = EvoHime.exe`. Supervisor передаёт
Core путь к этому executable, Core запускает его в родительском Job Object и
обменивается bounded typed JSON через stdio. Electron backend использует
ephemeral partition, sandboxed BrowserWindow и блокирует каждый HTTP(S)-запрос
по hostname/resolved IP; arbitrary CDP endpoint не поддерживается. При
отсутствии packaged executable capability возвращает typed `unavailable`.
Snapshot, screenshot и download возвращаются как bounded metadata/ArtifactStore
objects; upload читает только разрешённый artifact locator. UI получает только
session/revision/control/error/artifact metadata.

## Voice pipeline и ambient audio

Listener остаётся отдельным authenticated runtime с whisper.cpp и verified
manifest. Capture требует microphone capability и ambient policy; audio
таблицы не содержат PCM/blob, а transcript/utterance lifecycle ограничен
retention, forget и bounded provenance. Quiet hours, pause, deny и deletion
останавливают или удаляют данные до memory/proactivity gates; новый engine не
загружается без manifest/ABI/hash/package validation.

## Vision и document worker

Vision остаётся optional Core-owned capability. `vision/v1` принимает только
bounded явно переданный artifact с capability snapshot и возвращает typed
unsupported/resource/unknown/degraded statuses; visual output не имеет host
action authority. Worker backend отсутствует в базовом package, поэтому
`backend_unavailable` — штатный fail-closed результат. Evidence, OCR claims и
memory/RAG citations требуют redacted page/frame provenance.

## Persistent Analysis Kernel (план 28, реализован)

Analysis Kernel остаётся bounded вычислительной средой и не является security
authority: Core владеет `AnalysisKernelSessionV1`, лимитами, object metadata,
идемпотентностью и host-request validation. Durable metadata хранится в
`analysis_kernel_sessions`, `analysis_kernel_objects`,
`analysis_kernel_events` и `analysis_kernel_idempotency`; большие значения не
дублируются и должны адресоваться существующим ArtifactStore. Migration v38
добавляет эту схему транзакционно поверх текущей v37.

`KernelRuntime` поддерживает только `json_parse`, `json_select`, `csv_summary` и
`object_put`; `filesystem`, `network`, `shell`, `credentials`, raw tool и
прямое чтение artifact отклоняются typed error до эффекта. Runtime state
ephemeral и не сериализуется. Лимиты включают lifetime/idle timeout, request
rate и output budget; чувствительные inline markers fail-closed.

### Declared agent surface: `analysis_run`

Агент не получает доступ к `KernelRuntime` напрямую. Единственный путь —
объявленный инструмент `analysis_run` в `crates/tool-runtime`, чья схема и
host-контракт описаны ниже; второй dispatch в Core не создаётся.

- **Декларация.** `crates/tool-runtime/src/tools/analysis.rs` владеет id,
  описанием, пустым списком `Permission`, таймаутом и canonical input schema;
  `builtin_input_schema` отдаёт эту схему без заглушки `{"input":{...}}`.
  Обязательные поля — `operation` и `args`; `args` принимает bounded text
  (`json_parse`, `csv_summary`, не более 16 KiB) либо закрытый объект с
  `value`/`path` (`json_select`, `object_put`). `logical_name`, `type_hint` и
  `sensitivity` опциональны, `secret` среди sensitivity отсутствует. Схема
  намеренно не использует `allOf`/`if`/`then`/`oneOf`: провайдерские
  трансляторы объявлений их вырезают или отвергают, а несовпадение
  `operation`/`args` всё равно проверяет адаптер до эффекта.
- **Классификация.** `capability_class = "local_computation"`,
  `side_effect = ReadOnly`, `approval = Never` (прав нет),
  `workspace_scope = "task"`, `output_size_limit` = bounded inline размер,
  `retry_class = "bounded"`. Риск-классификатор относит вызов к `None`.
- **Host.** `crates/tool-runtime/src/kernel_host.rs` объявляет трейт
  `KernelHost` с единственным методом `execute(task, request)` и типы
  `KernelTaskBinding` (`task_id`, `session_id`, `workspace_root`),
  `KernelToolRequest`, `KernelToolResult`, `KernelToolObjectRef` и
  `KernelToolError` с stable typed кодами
  (`analysis_kernel_unavailable`, `..._limit_exceeded`,
  `..._forbidden_capability`, `..._crashed`, `..._sensitive_inline_payload`,
  `..._invalid_input`, `..._output_too_large`, `..._storage_failed`).
- **Отсутствие host.** `ToolRegistry` хранит `Option<Arc<dyn KernelHost>>` и
  builder `with_kernel_host`; без host инструмент зарегистрирован, но каждый
  вызов возвращает `ToolError::Execution("analysis_kernel_unavailable")`.
  In-process fallback, обходящий supervisor worker, отсутствует. `Clone`
  сохраняет host, потому что копируется `Arc`.
- **Каталог.** `catalog_preflight_input("analysis_run")` возвращает
  `{"operation":"json_parse","args":"{}"}`, поэтому side-effect-free
  инструмент проходит preflight и остаётся в авторизованном snapshot; probe
  проверяется той же схемой в Core tests.

Supervisor имеет allowlisted `evohime-analysis-worker` launch contract и
аутентифицированный Core command channel для `kernel_launch`, `kernel_execute`
и `kernel_stop`: фиксированный sibling executable, `trusted-local-1`,
очищенное окружение и отдельный kill-on-close Job Object с memory/CPU limits.
IPC добавляет commands 162–165, 291 (`ListAnalysisKernels`) и events 27–28;
Electron main/preload маршрутизирует bounded payload, а панель
`AnalysisKernelPanel` показывает только metadata projection (без значений
объектов и абсолютных путей), объединяя `analysis_kernel.list` и live events.

Core подключает `CoreKernelHost` к `ToolRegistry::with_kernel_host` поверх
общей с `IpcBridge` карты активных runtime, лениво создаёт одну сессию на
задачу (`session_init_lock`), проверяет `idempotency_key` через
`AnalysisKernelStore`, переводит `LimitExceeded`/`Crashed` в БД и останавливает
активные сессии задачи при завершении `AgentRun` в `TaskCoordinator`.

Core отправляет worker-запрос только после повторной admission-проверки,
использует typed result и переводит runtime в `Crashed` при transport failure;
на рестарте running manifests fencing-ятся без blind retry, а supervisor worker
останавливается. TaskCheckpoint/child handoff получают только явно выбранные
immutable checkpointable refs; ephemeral memory в handoff запрещена. Для
optional artifact/tool surfaces отсутствие capability даёт typed
`forbidden_capability`/`unavailable`, а не обход host bridge.

План 28 закрыт 30 августа 2026 года. Release evidence включает migration,
storage/runtime/IPC/replay/approval-boundary, real worker protocol, packaged
worker manifest и Core/supervisor fault smoke; raw values, credentials,
transcripts и абсолютные пути в bundle не попадают. При сбое reset/stop,
recovery fencing и optimistic revision не допускают blind retry.

### Guided Capability Recipes v1

Guided recipe catalog — фиксированный Core-owned индекс над существующими
workflow templates, а не второй граф, runner или provider/evaluation owner.
`CapabilityRecipeDescriptor` имеет стабильный id/version и canonical hash;
catalog содержит восемь категорий из плана 176. Запуск доступен только при
exact binding на прошедший registry validation workflow template. Recipe без
такого binding получает typed `Unsupported`; текущие model-comparison,
prompt-variants, tool-use и local-model-fit не симулируют отсутствующий runner.

Core preflight сверяет bounded inputs, выбранный workspace, граф, capability
grants, budgets, approval nodes и known revisions. Внешние model/context/tool
owners, которые не сохраняют exact revision, отмечаются как `NotPinned`; exact
replay и compatible rerun для них возвращают typed `unavailable`, а не
подставляют `latest`. `StartCapabilityRecipe` заново выполняет preflight и
создаёт существующий workflow run вместе с immutable
`capability_recipe_run_links` sidecar одной SQLite-транзакцией до запуска
effects. Sidecar хранит recipe/template/graph/input/workspace hashes и
idempotency key; lifecycle, cancellation и run recovery остаются за
`workflow_store`/`WorkflowRuntime`.

Authenticated additive IPC commands 280–284 обслуживают catalog, preflight,
start, run lookup и fork. Shell отображает только bounded Core projections в
существующей workflow builder surface и использует существующие workflow
status/cancel commands. Fork разрешён только для завершённого run и строит
новый user-owned draft из exact исходного template с placeholders; он не
копирует run inputs, outputs, grants, secrets или allowlists и не запускает
draft автоматически. Перед IPC startup Core вызывает recovery существующих
workflow runs; ошибка recovery не позволяет Core принять команды.

## Artifact Handoff Registry v1

`ProjectArtifactRegistry` is a Core-owned semantic layer over `ArtifactStore`.
SQLite schema v55 stores immutable artifact revisions, lineage edges, typed
handoffs, acceptance decisions and idempotency outcomes; it never duplicates
content bytes. Contract `artifact-handoff/v1` validates bounded metadata,
`artifact://` refs, producer/consumer role snapshots and lifecycle transitions.
Workspace and parent revision fingerprints drive selective freshness; unknown
scope is `possibly_stale`, and historical revisions are not rewritten.

Authenticated additive IPC command 205/event 54 and the Electron panel expose
bounded metadata only. Renderer cannot read SQLite/ArtifactStore, mint identity,
change capabilities, or supply secrets, prompts, raw outputs and executable
identities. Restart/recovery preserves durable registry state and does not
blindly repeat an optional consumer effect.

### Continual Refinement v1

План 29 закрыт 30 августа 2026 года. Core-контракт находится в
`crates/evohime-core/src/refinement.rs`: proposal создаётся только после
bounded independent evidence admission, получает immutable revision,
owner scope, provenance, policy snapshot hash и canonical content hash.
Authority-bearing тексты отклоняются, а Skill/PromptRule не активируются без
отдельного Core-owned activation adapter. Durable metadata/event storage
находится в `crates/evohime-local-storage/src/refinement_store.rs` и добавлен
транзакционно в schema v39; renderer не получает candidate body, transcript,
secret или hidden reasoning. Authenticated IPC commands 166–168 дают bounded
list/get/action projection с optimistic version checks. Electron OperationsPanel
только отображает состояние и отправляет явные действия; policy и effect
остаются в Core.

### Integration Provider SDK v1

План 33 реализован как Core-owned metadata contract. Typed manifest/action/
trigger/credential/binding/fixture types и bounded schema validator находятся в
`crates/evohime-core/src/integration_provider_sdk.rs`; из production adapters
поддержан read-only `github.public` для public repositories, а offline
`fixture.echo` доступен через `integration_provider_runtime.rs`. Metadata хранится в schema v40 в
`integration_provider_store.rs`; secret bytes, raw prompts и provider output не
записываются. Workflow binding использует version-pinned `integration_action`,
а неизвестные provider/action дают unresolved outcome. IPC остаётся
authenticated/additive: commands 175–176 и event 35; Electron показывает
metadata-only Integrations в `SettingsModal`. Unknown/unavailable outcomes
fail closed и не повторяют внешний effect вслепую.

### GitHub Public Repository Integration v1

Settings «Интеграции» позволяет сохранить до 50 идентификаторов публичных
репозиториев GitHub и по явному действию пользователя загрузить краткую сводку,
до 10 открытых issues и до 10 pull requests. Schema v180 добавляет
`github_saved_repositories` только с owner/repo и временем добавления; список
остаётся в SQLite Core и не хранит внешние ответы или credentials.

Core принимает только bounded owner/repo сегменты и обращается только к
`https://api.github.com` анонимными HTTPS GET к repository, issues и pulls
endpoints. Redirects отключены, каждый response ограничен 512 KiB, один refresh
за раз ограничен общим timeout 25 секунд; response body не читается. Issues,
содержащие `pull_request`, исключаются из issue list. `403`/`429`, not-found,
timeout, network и oversized/invalid response возвращаются как bounded error
codes без raw headers/body. Открытие Settings и добавление идентификатора не
делают внешних запросов; только явный выбор/обновление repository вызывает API.
В IPC используется существующий authenticated Integration Provider command
175–176 и event 35, без новых proto tags. Renderer отображает Core projection,
а внешние ссылки открывает через main-process allowlist. Private repositories,
OAuth/PAT и write operations не поддерживаются.

### Invocation Presets v1

`crates/evohime-core/src/invocation_presets.rs` содержит Core-owned typed
контракт version-pinned preset, canonical SHA-256 redacted payload, allowlist
валидацию и completed-run sanitizer. Durable immutable revisions находятся в
`crates/evohime-local-storage/src/invocation_presets_store.rs`; authenticated
IPC использует additive commands 179–180/event 37, а Electron показывает
metadata-only список, ручное сохранение текущих workflow inputs и запуск
preset. Automation schedule хранит immutable preset revision/hash/workspace
snapshot; due slot проверяет drift/rebinding и передаёт inputs в обычный
WorkflowRuntime. Migration — explicit Core operation с bounded mapping,
preview и новой immutable revision; temporary overrides не изменяют storage.

### Core-Owned A2A Remote-Agent Bridge v1 (plan 180, stage 1)

`crates/evohime-core/src/external_agent_a2a.rs` реализует Core‑owned outbound A2A remote‑agent backend как protocol/backend extension существующего external‑agent owner (`external_coding_agent_adapter`). Он определяет bounded untrusted profile/card/delegation/capsule/task‑binding contracts с deterministic validation, SHA‑256 хэшированием и детерминированными переходами состояний. Card claims никогда не дают локальных capability; сырые карты, заголовки, куки, транскрипты, токены и сырые цели никогда не попадают в persistent или projected структуры — сохраняются только id, ссылки, хэши и типизированное состояние.

Contracts:
- `RemoteAgentProfile` – версионированный, явно зарегистрированный профиль с endpoint boundary, auth‑profile reference, skill/capability allowlists, privacy ceiling и concurrency limit.
- `RemoteAgentCardSnapshot` – bounded, untrusted, хэш‑ограниченный снимок карты удалённого агента; сохраняет только хэши, bounded claims и типизированные флаги.
- `DelegationRequest` – детерминированная делегация, привязанная к точным хэшам профиля/карты, parent‑grant reference, capability, хэшированной цели, idempotency key и artifact budget.
- `ContextCapsule` – неизменяемая капсула делегации, содержащая только ссылки на context‑namespace/loadout, никогда сырые данные workspace.
- `RemoteTaskBinding` – durable binding между локальной делегацией и удалённой задачей, с состояниями жизненного цикла (`Registered`, `Submitting`, `Submitted`, `InputRequired`, `AuthRequired`, `Running`, `Completed`, `Rejected`, `Failed`, `CancelRequested`, `Cancelled`, `UnknownOutcome`, `Unavailable`) и reconciliation disposition (`Pending`, `ReconciledSameTask`, `Abandoned`).
- `A2aError` – ошибки валидации для контрактов профилей/карт/делегаций/капсул/задач.

Инварианты: remote card claims никогда не дают локальной authority; профиль, карта, делегация, капсула и binding контентно‑хэшированы; производный от контента privacy может только повышаться, никогда не понижаться; неоднозначная отправка становится `UnknownOutcome` и никогда не возвращается к слепой повторной отправке (`UnknownOutcome` может быть reconciliated только к известному исходу той же задачи).

`crates/evohime-local-storage/src/external_agent_a2a_store.rs` предоставляет metadata‑only persistence для профилей, хэшей снимков карт и bindings задач; полные карты, тела ответов, заголовки, куки, транскрипты, цели и токены никогда не хранятся. Store устанавливает schema v181 и является additive к существующему external‑agent owner. Миграция `v181.rs` создаёт три таблицы (`a2a_remote_profiles`, `a2a_card_snapshots`, `a2a_task_bindings`) и индексы.

`crates/evohime-core/src/external_coding_agent_adapter.rs` расширен версионированным A2A backend kind (`ExternalAgentProtocolKind::A2a { protocol_version: u32 }`) и валидацией preset, которая отклоняет неправильные версии протокола и несоответствующие метки протокола.

Все юнит‑тесты проходят; fmt/clippy clean. Никакой inbound HTTP/webhook listener не вводится, не создаётся второго runtime, scheduler, storage или team‑roster authority.

### Agent Middleware Pipeline v1

`crates/evohime-core/src/agent_middleware_pipeline.rs` содержит versioned
Core-owned contract для восьми фаз agent/model/tool loop. Встроенные policies
могут только наблюдать, сузить bounded request или заблокировать его; typed
immutable override сохраняет input hash и provenance, а capability snapshot
проверяется ещё раз перед effect. Ordering определяется `(priority, id)` и
snapshot run привязывает definition revision, contract/policy hash и capability
snapshot hash. `observability.rs` и `PolicyGate` остаются низкоуровневыми
примитивами и единственной final authority перед внешним effect.

`agent_middleware_pipeline_store` добавляет schema v43 с immutable definition и
run snapshot metadata; raw request/result и transient hook payloads не
сохраняются. Duplicate idempotency возвращает typed `Duplicate`, drift и
oversized input fail closed. Authenticated IPC использует additive commands
183–184/event 39, а Electron `AgentMiddlewarePipelinePanel` получает только
bounded metadata и отправляет действия через Core.
## Adaptive Tool Catalog v1 (план 38)

Adaptive selection — это Core-owned narrowing layer между `ToolRegistry` и
`ToolAgent`. Registry manifests и permission preflight формируют authoritative
snapshot; selector не может добавить capability, а перед моделью остаются
только schemas выбранных ids. Deterministic selector ранжирует bounded compact
metadata по нормализованным токенам; optional semantic/model selector возвращает
только stable ids и проходит ту же Core allowlist validation.

Catalog projection ограничен 32 tools (default 8), описанием 256 Unicode
символов и input schema 64 KiB. Empty/invalid/unavailable selection применяет
явный deterministic fallback. Cache живёт только в Core process, ключ включает
revision/registry/policy/grant/query/selector/limit и инвалидируется при любом
изменении этих входов или рестарте. Durable storage не добавляется; в журнал
попадает лишь bounded redacted selection metadata. Existing authenticated
`model.context` event — единственная client projection; Electron panel не имеет
доступа к Core, storage или full schemas.

Имена инструментов Core и имя функции в provider-запросе — один и тот же id:
`filesystem_read`, `git_status`. Провайдерские ограничения
(`^[a-zA-Z0-9_-]+$` у OpenAI и DeepSeek) выполняются самим форматом id, поэтому
шага преобразования имён больше нет. `wire_tools` в
`crates/model-gateway/src/providers/literouter.rs` снимает только Core-owned
`manifest_hash` и schema-ключи, которые не принимает upstream; имя функции он
не трогает. Обратный резолв `adaptive_tool_catalog::resolve_tool_call_name`
остаётся только для исторической точечной формы из старых prompt-кэшей и
работает лишь внутри активного loadout.

Легаси-совместимость держится на входных границах, а не на подмене перед
отправкой: `ToolRegistry::resolve_tool_id` принимает `git.diff` для durable
записей (receipts, continuation gates, старые IPC-клиенты),
`canonical_tool_id` нормализует списки в сохранённых capability/skill/profile
пакетах, а новое состояние пишется только в канонической форме.

Дедупликация повторов (`recovery::RecentToolCalls`) удаляет повтор
мутирующего вызова в окне шести сигнатур; успешная правка или коммит сбрасывают
это окно для read-only инструментов (`git_status`, `git_diff`,
`filesystem_read` и соседних), чтобы модель могла перечитать изменённое
состояние, не получая «повтор удалён Core».
## Structured Response Contract v1

Model Gateway предоставляет Core-owned schema-first structured response path.
`ResponseContract` содержит contract id, revision, JSON object schema, strategy
(`auto`, `provider_native`, `synthetic_tool`) и deterministic SHA-256 hash.
Размер schema ограничен 64 KiB; результат проходит локальную Core validation
по root type, required и property types. `Auto` выбирает native только для
маршрутов с capability descriptor, иначе использует synthetic output-tool.
Output-tool не является capability, не попадает в ToolRegistry и не выполняет
side effects. Общий лимит — 3 model attempts (не более 2 repair retries).

Lifecycle snapshot, policy hash, attempts и unknown-after-restart состояние
ephemeral; новые SQLite tables/migrations не добавляются. Provenance хранит
только redacted contract hash, strategy, provider capability и typed outcome.
Незавершённый provider request после restart не повторяется автоматически.

Desktop IPC остаётся authenticated и additive: `StructuredResponseCommand` —
tag 185, `StructuredResponseEvent` — tag 40. Electron получает только
bounded metadata projection через generic shell bridge; raw schema, prompt,
output и repair text в renderer не передаются.

## Execution Backend Registry v1 (план 43)

`evohime-core::execution_backend_registry` — Core-owned registry execution
окружений. Встроенный `local.core` является безопасным default; remote-записи
содержат только canonical HTTPS endpoint и credential reference. Реального
remote transport или запуска внешнего процесса этот контракт не добавляет:
remote handshake возвращает typed `transport_unavailable`.

Контракт version 1 ограничивает 64 backend и 64 capability ids, валидирует
lowercase ids и запрещает userinfo/query/fragment/private или loopback endpoint.
Lifecycle health — `registered/probing/healthy/degraded/unavailable/disabled`,
ошибки типизированы (`invalid_endpoint`, `incompatible_contract`,
`capability_denied`, `transport_unavailable` и др.). Advertised capabilities
пересекаются с Core policy и не могут расширить grants.

Metadata хранится в SQLite schema v45 (`execution_backends` и bounded event/meta
tables) через additive backup-before-migrate path. Mutations используют
optimistic version и idempotency envelope; UI получает только bounded list,
health, capability count и наличие auth ref. Active run affinity фиксируется
immutable `{backend_id, registry_version, handshake_hash, policy_hash}` snapshot
и не меняется при смене default. Unknown external outcome не retry/failover.

Authenticated additive IPC использует command 189/event 44. Electron panel
«Среды выполнения» является projection/action surface; prompt, output, secret
material, executable identity и raw endpoint payload в renderer не передаются.

## Tool Simulation Runtime v1 (план 44)

`tool_simulation_runtime.rs` — Core-owned interception boundary для безопасного
fixture/emulated dry-run. Explicit modes — `real`, `fixture`, `emulated` и
`dry_run`; Real не перехватывается этим runtime, а simulation modes никогда не
вызывают `ToolRegistry` effect adapter и не fallback-ятся в Real. Exact fixture
matching использует schema v1, tool id и SHA-256 hash нормализованного JSON input.
Fixture и Emulated output проходят bounded limits и optional Structured Response
validation. Provenance типизирован как `fixture` или `synthetic`, поэтому
synthetic evidence не является observed effect.

Run, fixture и policy state process-local и исчезают после restart; SQLite
schema остаётся v45 без новых таблиц. В журнал допускаются только bounded
redacted metadata. Duplicate delivery idempotent, missing fixture и invalid
schema fail closed. `CoreNodeAdapter::with_simulation` intercepts tool nodes
после Core policy recheck и возвращает только fixture output; benchmark matrix
использует `FixtureToolBenchmarkExecutor` только для `fixture:` references.

Authenticated additive IPC использует command 190/event 45. Electron получает
только metadata-only status с mode/state/provenance и счётчиками ephemeral
runtime; raw fixture/input/output, prompts, credentials и executable identities
не пересекают boundary. Панель всегда показывает, что simulation не
подтверждает реальный эффект.

## External Coding Agent Adapter v1 (план 45)

`evohime-core::external_coding_agent_adapter` определяет bounded protocol
`evohime.external-agent/v1`: newline-delimited JSON frames `hello`, `hello_ack`,
`run`, `event`, `result`, `cancel`. Core валидирует manifest, capability
intersection, declared credential slots, timeout и immutable `AgentSnapshot`;
raw prompts, outputs, credential values и executable paths не входят в desktop
IPC.

Preset/conversation/event metadata хранится additive в SQLite schema v46;
process handles, streams и transient run state остаются ephemeral. Core
передаёт supervisor только validated opaque run spec. Supervisor разрешает
executable через allowlisted environment mapping, запускает без shell, назначает
отдельный Windows Job Object и уничтожает дерево при cancel/timeout. Unknown
outcome после restart не retry-ится вслепую.

Authenticated additive IPC использует commands 191–192/event 46. Electron
получает только bounded metadata с `core_control_level`
(`full`/`supervised_opaque`/`unavailable`); raw external frames не пересекают
desktop boundary.

## Agent Client Protocol Bridge v1 (план 169)

ACP является typed transport/interop расширением существующего external-agent
owner: `ExternalAgentProtocolKind::Acp { protocol_version: 1 }` не меняет
старый `EvoHimeV1` и по умолчанию сохраняет `ExternalAgentBackend`. Core
валидирует bounded stdio/framed JSON-RPC, correlation, handshake,
capability snapshot и state transitions; malformed, oversized, unsupported,
stale, interrupted и unknown outcomes остаются non-success. Auth mode
`ExistingLocalSession` проверяет только documented readiness и не читает,
копирует или сохраняет чужие token/cookie/session files.

ACP session projection в schema v169 хранит только revision/hash, identity,
capabilities, auth/control/privacy state и provenance metadata. Запуск pin-ит
точный preset/executable/capability snapshot, а restart не re-adopt-ит процесс
по одному PID. Opaque/observed tools не получают ложную CoreMediated approval;
unknown privacy блокирует Restricted/Sensitive, LocalOnly запрещает
cloud-backed subscription, а FreeOnly не считает paid subscription бесплатной.

Authenticated additive IPC использует command 277/event 122. Electron
`ExternalCodingAgentAdapterPanel` показывает только Core-derived protocol,
backend, auth, capability hash, control/privacy state и next-action metadata;
raw ACP frames, stderr, paths, workspace payload и credentials не пересекают
desktop boundary.

## Schema-Driven Agent Configuration v1 (план 67)

Core публикует versioned `ConfigurationSchema` с пятью слоями
`ApplicationDefaults`, `WorkspaceDefaults`, `AgentProfile`,
`ConversationDefaults`, `RunOverride`. Поля имеют typed kinds, Core registry
source, sensitivity и apply/restart semantics; semantic patches проверяются
Core, а неизвестные executable-like fields и недоступные references отклоняются.
`ConfigurationSnapshot` содержит effective values, source layers, redacted
secret states и deterministic `sha256` hash. Snapshot revision fenced
optimistically и сохраняется metadata-only в SQLite schema v65; обновление не
мутирует active run и не запускает side effects. Authenticated additive IPC
использует command 218/event 63, Electron получает только bounded projection.

## Declarative Agent Component Registry v1 (план 74)

Core-owned registry использует stable public provider IDs, typed
`ComponentDescriptor`, отдельные spec/component versions и built-in trusted
allowlist. Loading выполняет provider/type/version/schema/dependency validation;
unknown providers, cycles, missing migrations и raw secret values fail closed.
Descriptor diff/dump и explicit one-step migration остаются deterministic и
bounded; dynamic code loading и marketplace отсутствуют. Durable registry
хранится в SQLite schema v71, authenticated additive IPC использует command 225
/ event 70, а Electron показывает только metadata/action projection.

## Headless Core CLI v1 (план 77)

Официальный `eva` — authenticated headless-клиент существующего Core, а не
второй agent runtime. На Windows он подключается к supervisor-owned Core через
named pipe; на Linux подключается к автоматически запущенному Core через
owner-only Unix domain socket. Linux runtime-каталог находится в
`$XDG_RUNTIME_DIR/evohime` или `$HOME/.cache/evohime`, имеет режим `0700`,
сокет — `0600`, а handshake проверяет kernel peer UID и HMAC session proof с
ролью `cli`. На обеих платформах CLI переиспользует Core `StartTask`,
`StartWorkflow`, status, cancel и event/replay contracts; SQLite и model/tool
runtime CLI напрямую не открывает.

Контракт `evohime.cli.event/v1` ограничивает prompt, workspace и event
projections, отделяет stdout NDJSON от stderr diagnostics и redacts secrets,
credentials, prompts, tokens, hidden reasoning и raw output. Поддерживаются
human/one-shot, `--json` NDJSON, bounded stdin, detached acceptance, watch с
reconnect от последнего sequence, status, cancel и `resume`; последний
безопасно продолжает наблюдение за существующим run через тот же event cursor,
не перезапуская агент и не повторяя effect. Exit codes versioned (0–8), а
Windows `eva.exe` включён в native package без Node/Python runtime. Linux CLI и
Core собираются из исходников командами Cargo; desktop shell и
supervisor-dependent Windows features от этого не становятся Linux-продуктом.

Framing, HMAC-handshake и command envelopes CLI вынесены в отдельный
платформенно-независимый crate `evohime-cli-protocol`, а bounded `RunRequest`
и terminal-event contract живут в лёгком общем crate
`evohime-cli-contract`.

## Capability Workbench v1 (план 78)

Capability Workbench — Core-owned lifecycle-scoped runtime component, а не
renderer/plugin runtime. Versioned `WorkbenchDescriptor` ограничивает kind,
scope (`RunScoped`, `GoalScoped`, `ProjectScoped`, `UserSessionScoped`),
concurrency (`Exclusive`, `Serialized`, `Parallel`), tools и shared resources.
Границы первой версии: IDs до 128 bytes, до 128 tools, 64 resources, 32
leases/in-flight calls и portable snapshot до 256 KiB. Capability grants
фильтруют discovery и проверяются повторно перед call.

Состояния `Created → Starting → Ready → Stopping → Stopped` дополняются
`Resetting`, `Degraded` и `Failed`; переходы revision-fenced. Durable SQLite
schema v74 хранит только сериализованный logical instance, snapshots и
resource leases. Heartbeat продлевает lease, истечение переводит instance в
`Degraded`, очищает in-flight accounting и допускает reconciliation/recovery.
Portable snapshot содержит только безопасное logical state и credential refs:
raw credentials, prompts/outputs, OS handles и executable identities запрещены.

Core command 228/event 73 предоставляет create/get/start/stop/reset,
list_tools, typed call result, snapshot/restore, heartbeat, resource
availability и cancellation outcome (`cancelled`, `already_terminal`,
`unknown`). Неизвестные Core surfaces возвращают typed unavailable/degraded;
arbitrary backend не загружается. Electron developer panel — bounded
projection и маршрутизация, без state machine, DB или effects в renderer.

## Durable Remote Task Bridge v1 (план 98)

`RemoteTaskToolset` и `RemoteTaskRecord` принадлежат Core и хранят только
opaque task identity, provider/toolset ref, request hash, lifecycle status,
transport status, lease/poll counters, provenance и optional artifact ref.
Provider-neutral toolsets бывают MCP или Integration Provider; bridge не
исполняет transport и не хранит credentials или raw provider payload. Typed
операции `submit`, `status`, `poll`, `result` и `cancel` проходят bounded
validation, monotonic version fence и Core event journal. Poll lease ограничен
32 попытками; `Completed` без artifact ref отклоняется, `Unknown` не считается
успехом и не запускает blind retry. Durable metadata хранится в
`remote_task_records`, а authenticated IPC использует command 251/event 96 и
metadata-only Electron projection.

## Agent Git Change Sets v1 (план 102)

Core хранит bounded `AgentGitChangeSet` с exact baseline (`HEAD`, dirty
fingerprint и workspace binding), классификацией путей и immutable
`GitCommitCandidate`. По умолчанию в candidate попадают только
`AgentAuthored` и `GeneratedByApprovedTool`; pre-existing, external, secret и
ambiguous paths исключаются. Renderer получает только redacted projection.
Контракт ограничен 256 путями, candidate — 128 путями, message — 4 KiB,
evidence — 64 KiB. Shared index, `git add -A`, force/reset/rebase/push и
неподтверждённые commit/undo effects запрещены; stale/ambiguous outcomes
возвращаются типизированно.

`observe` захватывает состояние непосредственно из Git, а `candidate` повторно
сверяет workspace и строит bounded path/hash precondition. `commit` выполняет
свежий preflight, точечно stage-ит только approved paths и затем использует
`git commit --only` с NUL-delimited pathspec; общий index, `git add -A`,
force/reset/rebase/push и подмена identity запрещены, hooks не обходятся. `keep`,
`reconcile` и `undo` являются
отдельными Core-owned действиями; undo меняет только подтверждённые пути и отказывается
при stale/shared/unknown outcome. Перед commit durable candidate получает
`commit_pending`; dispatch/persistence crash-window переводит повторный запрос
в reconciliation-required и не допускает blind retry. После Git-эффекта
результат сначала сверяется с новым HEAD, а ошибка durable persistence также
возвращается как reconciliation-required. `reconcile` не повторяет Git effect:
по parent/message/path evidence он фиксирует только доказанный commit,
возвращает candidate в preflight только при доказанном отсутствии effect,
иначе переводит его в `unknown`.

При работе из Incremental Change Protocol или Task Worktree Isolation payload
может нести durable `incremental_change_run_id` и `task_worktree_id`. Core
разрешает такие ссылки только по существующим storage records: terminal
incremental runs отклоняются, а worktree обязан быть в `ready`/`integrating`
и иметь тот же base HEAD. Поэтому эти consumers используют тот же baseline,
attribution и stale contract, а не второй механизм Git authority.

Каждая мутация несёт idempotency key и optimistic revision. Core сначала
durable claim-ит ключ, поэтому конкурентный duplicate не запускает второй
effect; завершённый запрос возвращает сохранённый redacted результат, а
pending claim остаётся reconciliation-required. Workspace root принимается только
через authenticated Core command и проверяется как Git worktree с
Core-derived binding. Контракт ограничен 256 путями, candidate — 128 путями,
message — 4 KiB, evidence — 64 KiB, а IPC-поля дополнительно ограничены
UTF-8 byte limits (workspace root — 32 KiB, operation references — 256 bytes).
Git subprocesses имеют bounded timeout 120 секунд; timeout или ошибка после
dispatch effect считаются unknown outcome и не ретраятся вслепую. Storage
migration v94 добавляет revision и durable idempotency outcomes. Используются additive authenticated IPC command 233 и
event 78; generated bindings и Electron panel остаются metadata-only.

## Stateful Tool Workbench Sessions (план 103, reuse плана 78)

Stateful tool collections используют существующий Core-owned Capability
Workbench contract: `WorkbenchDescriptor` задаёт scope/concurrency/tools,
`WorkbenchInstance` — lifecycle/revision/in-flight state, а leases и snapshots
хранятся в SQLite. Snapshot принимает только bounded logical state и credential
refs; process handles, secrets и private backend state исключены. Reset/restart,
stale revision, capability denial, lease expiry и unknown/unavailable outcomes
обрабатываются Core handler-ом и metadata-only Electron projection. Новый
параллельный authority для плана 103 не создаётся.

## Reasoning Operator Library v1 (план 83)

Core хранит versioned definitions для Generate/Review/Revise/Ensemble с
bounded typed input/output contracts и максимумом трёх попыток. Operators
являются model-computation primitives: они не получают tools, grants или
mutation authority; результаты остаются proposals до обычных Core boundaries.
SQLite schema v82 и authenticated additive IPC command 236/event 81 хранят
только metadata и redacted projections.

## Customization Inventory v1 (план 85)

Core-owned normalized metadata catalog объединяет Skill, Integration, Profile,
Workflow и UiExtension без объединения runtime semantics. Schema 84 и
authenticated additive IPC tags 238/83 поддерживают bounded list/register/remove;
owner registries сохраняют authority, grants и side-effect policies. Renderer
видит только redacted metadata projection.

## Remote Conversation Channels v1 (план 104)

Core-owned `ChannelConnection` фиксирует provider, owner scope, external
identity, lifecycle revision и bounded queue/attachment limits. Pairing code
хэшируется, имеет TTL и single-use durable claim; каждое inbound message
проверяет identity, active/revocation state и dedup. Provider credentials
представляются только opaque `credential_ref`; high-risk remote approval
требует desktop grant. IPC 246/event 91 и Electron projection metadata-only;
provider dispatch остаётся adapter boundary без DB/workspace authority.

## Declarative Runtime Components v1 (план 106)

`ComponentConfig` является versioned data-only envelope поверх существующего
Core Component Registry/provider descriptor из плана 74. DefinitionConfig,
RuntimeState и SecretBindings разделены; секреты представлены только opaque
credential refs. Canonical SHA-256, bounded limits и explicit 0-to-1 migration
делают конфигурацию пригодной для provenance. Перед rehydration Core повторно
проверяет registry, policy version и capability allowlist; unknown outcome
переходит только через reconciliation и не объявляется успехом автоматически.
Durable configs сохраняются в schema 90 с optimistic revision/idempotency fence.
Authenticated IPC 248/event 93 и Electron panel передают только redacted
metadata; provider registry, storage и effect authority остаются Core-owned.

## Code-Anchored Intent Markers v1 (план 114)

Core принимает только типизированные comment ranges и связывает каждый
`CodeIntentMarker` с логическим путём, точной revision и диапазоном строк.
Поддерживаются `EditRequest` (`EVA!`) и `Question` (`EVA?`); marker содержит
bounded text, content hash, optional symbol и provenance. Сканирование
существующего репозитория инертно, ограничено ephemeral debounce/dedup gate и
не выдаёт право на запуск. Только отдельное явное действие пользователя
проходит trusted provenance и передаёт prompt в обычный `StartTask` путь.

IPC command 255/event 100 additive и metadata-only: raw prompt не возвращается
в renderer projection. Пути traversal, stale revision, oversized ranges,
неизвестные операции и untrusted provenance fail closed. Marker не создаёт
capability/approval, не исполняет код комментария и не хранится в SQLite.

## Execution Environment Profiles v1 (план 119, реализован 2026-09-08)

`ExecutionEnvironmentProfile` — Core-owned bounded metadata envelope для
композиции ссылок на authoritative owners. Он содержит только id, revision,
scope, typed bindings, reference mode, required-флаг и canonical SHA-256;
произвольные поля, credentials, prompts, outputs, executable paths и owner
payloads отвергаются либо не допускаются схемой. `PinnedRevision` фиксирует
revision, а `FollowCompatible` сохраняет фактически разрешённую revision в
effective snapshot.

В SQLite schema v93 добавлены профиль и immutable revision history,
activation history/current snapshot, run-pinned snapshot и bounded
idempotency outcomes. Revision fence, размер записи 64 KiB и одна транзакция
для activation/current не допускают частичного переключения. Повторная
привязка run сохраняет первый snapshot (`INSERT OR IGNORE`), поэтому активная
среда не меняется внутри уже созданного run. Startup recovery только
валидирует сохранённые canonical records и не объявляет неизвестную среду
успешной.

Resolver остаётся adapter boundary: в этом checkout есть безопасные metadata
lookups для Model Purpose Routing, Execution Backend Registry, External Coding
Agent Preset, Execution Policy и Approval Policy. Для Workbench, MCP Server,
Skill Set, Instruction Stack, Continuation/Budget Policy и Credential Binding
отсутствие versioned owner lookup даёт typed `unavailable_owner`; обязательная
ссылка делает профиль `Broken`, необязательная — `Degraded`. Это намеренный
fail-closed результат проверки владельцев, а не копия их state в профиле.
Required failures блокируют activation; drift pinned revision даёт
`NeedsReview`; effective snapshot разрешён только в `Ready`/`Degraded`.
Backend, external agent, workbench, MCP и credential changes получают
`NewRunOnly`, остальные безопасные metadata changes — bounded `NextTurn`.

Authenticated additive IPC использует command 260/event 105. Command содержит
operation, owner scope, profile id, bounded payload, expected revision и
idempotency key; Core проверяет canonical hash, optimistic revision и
idempotency command hash/conflict. Replay/resync восстанавливает typed event,
а Electron получает только bounded metadata projection через `Профили сред`.
Renderer не является источником effective state и не получает secret-like
unknown fields.

## Remote Client Control Plane v1 (план 127, MVP реализован 2026-09-09)

`crates/evohime-remote` — канонический bounded frame/device contract: protocol
version 1, operation, immutable request/idempotency keys, sequence, optional
device/text payload и SHA-256 content hash. `SequenceGuard` fail-closed
отвергает gap/replay. Core boundary не открывает внешний transport и публикует
`Unavailable` до authenticated deployable relay; Android/server adapters не
получают authority, secrets или SQLite access. Реальный relay, Android APK и
server installer отсутствуют в checkout и остаются отдельным красным gate.

## Built-in Deterministic Developer Utilities v1 (план 133, реализован 2026-09-09)

`evohime-tool-runtime::developer_utilities` содержит curated stateless provider
для bounded локальных преобразований: Base64 encode/decode, SHA-256/SHA-512,
JSON format/minify, UUID v4, OS-CSPRNG token и case conversion. Каждый utility
имеет отдельный стабильный tool ID, typed manifest schema, версию, implementation
revision, `local_computation` capability metadata и пределы 256 KiB input /
512 KiB output. Pure операции дают одинаковый результат для одинакового input;
UUID/token явно маркированы `secure_random`, а random value не является
telemetry payload.

Provider зарегистрирован в обычном `ToolRegistry` и вызывается тем же Core
dispatch/permission/approval/receipt/event/recovery путём, что и остальные
инструменты. Он не создаёт scheduler, Workbench, SQLite authority, lease,
filesystem/network/shell capability или отдельный IPC dispatcher. Ошибка
декодирования, JSON parse, unsupported option, oversized input/output и
неизвестный tool ID завершаются non-success. Renderer и workflow используют
тот же Core-owned manifest/result contract; raw secret material не пишется в
utility metadata.

## Kernel Capability Facade v1 (план 139)

`kernel_capability_facade` хранит bounded snapshot-bound capability entries,
programmatic call identity и durable handle metadata. Snapshot — projection
разрешений, не grant store: every call revalidates exact snapshot, capability
version и availability against existing Core policy/approval/tool/workflow/
child/context owners. Migration v108 хранит только metadata; facade не
исполняет shell/network/code и не создаёт второй registry. IPC 267/112 и
Electron показывают redacted status/provenance.

## Runtime Service Graph v1 (план 141)

`runtime_service_graph.rs` владеет только bounded описанием связей runtime:
immutable revision/hash, lifecycle, scope, nodes и edges. Core валидирует
уникальность узлов, границы и canonical SHA-256; SQLite schema v110 хранит
только revisioned metadata и idempotency key в транзакционной migration с
обычным backup boundary. `pin` проверяет active graph и закрепляет revision
для run, но не исполняет node и не создаёт scheduler, permission system или
внешний service.

Authenticated additive IPC command 269/event 114 даёт replay-safe metadata
projection: статус, revision, hash prefix, counts, lifecycle и typed errors.
Electron renderer остаётся projection-only; secrets, prompts, logs, executable
identities и authority через этот контракт не проходят. Invalid, stale,
unknown и unsupported lifecycle transitions fail closed; effect ownership
остаётся у существующих Core policy, approval, workflow и runtime owners.

## Agent Program Optimizer v1 (план 142)

`agent_program_optimizer.rs` хранит bounded metadata-only программу с
immutable revision/hash, lifecycle, objective hash и шагами без prompt/body.
Core проверяет уникальность шагов, размеры и canonical SHA-256; schema v111
хранит revisioned program metadata, idempotency и durable run pin в
транзакционной migration. `optimize` выдаёт deterministic score/verdict и не
исполняет шаги, не меняет workflow и не создаёт второй scheduler или gateway.

Authenticated additive IPC command 270/event 115 проецирует только status,
revision, score, counts, hash prefix и typed failures. Renderer не получает
authority, secrets, raw prompts/outputs или executable identities; stale,
unknown, conflict и unsupported lifecycle transitions fail closed.

## Git Remote Publication Protocol v1 (план 145)

`git_remote_publication_protocol.rs` хранит bounded intent для remote ref,
branch, commit hash, scope и immutable revision. Schema v113 хранит только
metadata и idempotency. `inspect/publish` остаются typed
`transport_unavailable`: внешний Git transport, credentials и push authority
не добавлены; существующий Core Git/change-set owner остаётся единственным
effect owner.

Authenticated additive IPC command 272/event 117 даёт redacted projection с
hash prefix, status и typed failure. Renderer не получает secrets, raw Git
output или authority; unknown/stale/denied outcomes не объявляются success.

## Voice Input & Dictation v1 (план 146)

`voice_input_dictation.rs` хранит bounded profile metadata: locale, adapter,
scope, lifecycle и immutable revision/hash. Schema v114 хранит только
revisioned metadata и idempotency; raw audio/transcript не сохраняются. Core
availability fail-closed сообщает `unavailable` до подтверждённого listener
runtime evidence, не создавая второй audio owner.

Authenticated additive IPC command 273/event 118 проецирует только bounded
availability и adapter metadata. Renderer не получает audio, transcript,
secrets или authority; unknown/unsupported adapter состояния не являются
success.

## Plans 149–167 — Core capability contract batch

Каждый закрытый контур получил отдельный bounded Core-owned record с
canonical SHA-256, lifecycle и revision-aware validation, а также отдельную
SQLite migration/store. Все projections metadata-only и fail-closed; внешние
устройства, сеть, IDE и voice adapters не получают implicit authority.

## Language Intelligence Runtime v1 (план 171)

Core владеет bounded registry/session/query/proposal metadata для managed LSP.
Descriptor допускает только зарегистрированный stdio provider с проверенным
trust boundary; session привязана к workspace/worktree и generation, query и
semantic result — к exact document/provider revision и content hash. LSP
WorkspaceEdit является proposal; raw JSON-RPC, arbitrary executable,
automatic command execution и второй diagnostics store запрещены. Schema v171
хранит только redacted metadata, а authenticated IPC 279/124 и Electron
projection не расширяют filesystem grants или authority renderer.

## Core-owned image generation and editing v1

Image generation and editing use the existing ModelGateway route, provider
profile, privacy policy, Core execution and ArtifactStore. Gateway advertises an
additive `image_output` capability with provenance, supported operations, epoch
and bounded MIME/byte/dimension/pixel/count limits; missing or stale capability
fails closed. Core stores only bounded job metadata, request/result hashes,
artifact refs and typed lifecycle reasons in schema v177. Prompt and image
bytes are not persisted in job metadata or projected over IPC.

Generate, edit and mask-edit jobs validate caller-owned inputs and decode PNG or
JPEG through Windows Imaging Component under byte and pixel bounds. Provider
output is checked against declared MIME, container signature, decoded geometry,
aggregate limits and SHA-256 before a single quota-checked ArtifactStore batch
publishes the references. URLs are never fetched implicitly. Authenticated IPC
projects job status and artifact metadata; Electron's Operations panel invokes
Core and does not call providers or write workspace files. A dispatched job
whose outcome cannot be reconciled recovers as `unknown_outcome` without blind
retry; completed artifact refs are revalidated when read. Export remains an
explicit existing user-authorized action.
