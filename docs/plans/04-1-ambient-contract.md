# Этап 04.1: Контракт ambient и приватность

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: нет. Этап пишет side-effect-free контракт и capability поверх уже
существующих `crates/permissions` и `permission_rules`.

Разблокирует: все остальные этапы плана 04.

## Что этап отдаёт наружу

Единый bounded контракт ambient-слушания: состояния, лимиты, схема политики,
capability микрофона и правила логирования.

## Что уже есть в коде

Ambient-контракт и capability ещё не реализованы: в текущем checkout нет
аудио-крейта, ambient-веток в permissions и ambient-раздела в канонической
архитектурной документации. Прототип на Web Speech API удалён вместе с
веб-фронтендом и к native-архитектуре неприменим.

Сверено с кодом на момент написания этапа:

- `Permission` (`crates/permissions/src/lib.rs:23`) — восемь вариантов,
  `#[serde(rename_all = "snake_case")]`;
- `PermissionEngine::new()` (`lib.rs:240`) заполняет карту дефолтов поимённо,
  а `mode()` (`lib.rs:266`) для отсутствующего ключа возвращает **`Ask`**;
- `set_all_modes` (`lib.rs:279`) перечисляет те же восемь вариантов вручную;
- `PolicyRule` (`crates/permissions/src/policy.rs:5`) — `{permission, pattern,
  mode}`, `resolve` берёт **последнее** совпавшее правило;
- `crates/evohime-core/src/permission_rules.rs` только загружает
  `permissions.json` и применяет его; вариантов `Permission` он не
  перечисляет. Важно: при ошибке разбора файл отбрасывается **целиком** и
  подставляются `PolicyRuleSet::defaults()` (`permission_rules.rs:45`, `:59`);
- глобальные режимы в Core **не персистятся**: док-комментарий `lib.rs:3` про
  `app_settings.permissions` устарел, такой таблицы в коде нет. Долговечен
  только `permissions.json`;
- зато Electron хранит режим в workspace-store и переотправляет команду при
  каждом открытии воркспейса (`shell-bridge.ts:49`, `:146`, `:161`), то есть
  `set_all_modes` вызывается регулярно, а не однократно;
- `StructuredLogger::write(level, event, fields: serde_json::Value)`
  (`crates/evohime-core/src/logging.rs:26`) принимает произвольный JSON;
- образцы для стиля: `evohime-supervisor::schedule_contract` (side-effect-free
  контракт) и `crates/evohime-core/src/run_policy.rs:1` («Immutable bounded
  policy … The renderer may display the snapshot, but cannot increase a limit
  mid-run»).

## Содержание

- Новый крейт `crates/evohime-listener-contract` (вся семья листенера носит
  префикс `evohime-`: `evohime-listener-contract`, `evohime-listener-ipc`,
  `evohime-listener-audio`, `evohime-listener`) в стиле
  `evohime-supervisor::schedule_contract`: без файловой системы, часов и
  процессов. В нём:
  - `ListeningState`: `Stopped`, `Starting`, `Listening`, `PausedByUser`,
    `PausedByPolicy`, `DeviceConflict`, `DeviceDisconnected`,
    `EngineUnavailable`, `Denied` и допустимые переходы;
  - `AmbientLimits`: кадр 30 мс, pre-roll 300 мс, hangover 700 мс, минимум
    400 мс, потолок высказывания 20 с, эпизод 10 мин, окно дедупликации 60 с;
  - `AmbientPolicy` v1 (пауза, тихие часы, чёрные списки процессов и заголовков
    окон, retention) с валидацией и потолками длины. В тихие часы поток
    захвата полностью закрыт: высказывания не распознаются и не сохраняются;
  - `ProactivityBudget` — неизменяемый снимок лимитов (3/час, 10/сутки,
    10 минут между предложениями) по образцу `run_policy.rs`. Текущие счётчики
    и время последнего предложения не входят в snapshot: они живут в отдельной
    структуре `AmbientProactivityRegistry` в `evohime-core` — по образцу
    `RoutingApprovalRegistry` (`lib.rs:3777`, одно поле `Arc<Mutex<…>>`), а не
    в несуществующем «Core-состоянии». Долговечная часть счётчиков персистится
    таблицей миграции v26 из 04.7;
  - закрытый набор кодов ошибок.
- `Permission::MicrophoneListen` (serde `microphone_listen`) добавляется к
  восьми существующим вариантам `Permission`. Дефолт `Deny` прописывается явно
  в карте `PermissionEngine::new()`: fallback `mode()` — `Ask`, поэтому «просто
  не добавить» значит «спрашивать», а не «запрещено». `set_all_modes` обязана
  исключать `MicrophoneListen` — иначе смена общего режима молча откроет
  микрофон. Это не однократный риск: Electron переотправляет сохранённый режим
  при каждом открытии воркспейса, а ветка `PermissionMode` в
  `ipc_bridge.rs:1338` вызывает `set_all_modes` для **любого** значения
  (`full` → `Allow`, `read_only` → `Deny`, всё остальное → `Ask`), так что без
  исключения микрофон получал бы `Allow` или `Ask` при каждом старте.
  Исключение делается ровно в одном месте — в `set_all_modes`; `ipc_bridge.rs`
  править не нужно, он не адресует микрофон поимённо. На это ставится отдельный
  негативный тест по всем трём режимам.
- `permissions.json.example` — список правил вида `{permission, pattern, mode}`,
  поэтому ambient-правило записывается как
  `{"permission": "microphone_listen", "pattern": "*", "mode": "deny"}`.
  Правило и вариант enum обязаны попасть в **один коммит**: неизвестное имя
  разрешения ломает разбор всего файла, и `permission_rules.rs` молча
  откатывается на `PolicyRuleSet::defaults()`, теряя все пользовательские
  правила. Поскольку глобальные режимы Core не персистит, `permissions.json` —
  единственный долговечный канал запрета. Capability проверяется Core перед
  выдачей listener-разрешения, а не только в UI.
- Правила логирования. `StructuredLogger::write` принимает **произвольный**
  JSON — навесить на него allow-list нельзя. Поэтому ambient-логирование идёт
  через типизированный фасад: набор полей — фиксированная структура, а сырой
  логгер из ambient-пути недостижим. Свободного текста в фасаде нет по типам,
  поэтому «произвольный текст» — ошибка компиляции, а не падение теста;
  проверяемое утверждение формулируется как «сериализация любого варианта
  ambient-события не содержит полей вне allow-list». Логгер Core пишет только в
  `<data_dir>/logs/core.jsonl` (`main.rs:179`, `permission_rules.rs:68`) и при
  неудаче открытия завершает процесс; временного `%TEMP%`-фолбэка у него **нет**
  (путь `%TEMP%\evohime-log-<pid>.jsonl` встречается лишь в юнит-тесте
  `logging.rs:54`). Свой файл журнала листенер определяет в 04.3, и
  privacy-тесты 04.3 обязаны проверять именно его. Текст, хеш текста, имя
  процесса и заголовок окна в логи не попадают никогда — короткую фразу по хешу
  перебирают за секунды, поэтому хеш приравнивается к содержимому.
- Раздел «Ambient listening» в `docs/architecture.md` как канонический контракт.

## Файлы

- создать: `crates/evohime-listener-contract/{Cargo.toml,src/lib.rs}`;
- изменить: `Cargo.toml` (workspace members), `Cargo.lock` (тем же коммитом —
  CI идёт с `--locked`), `crates/permissions/src/lib.rs` (вариант enum, дефолт
  в `PermissionEngine::new()`, исключение в `set_all_modes`, негативные тесты),
  `permissions.json.example`, `.github/workflows/windows.yml` (добавить
  `-p evohime-listener-contract` в строку `cargo test`: CI перечисляет пакеты
  поимённо, и новый крейт иначе не собирается и не тестируется),
  `docs/architecture.md`;
- правок не требуют: `crates/evohime-core/src/ipc_bridge.rs` и
  `crates/evohime-core/src/permission_rules.rs` — оба работают с `Permission`
  обобщённо.

## Проверки

- перебор всех переходов `ListeningState`: недопустимый переход отвергается;
- политика с чрезмерным regex, слишком длинным списком или retention > 90 дней
  отвергается до применения;
- `MicrophoneListen` присутствует в карте `PermissionEngine::new()` со
  значением `Deny` — именно присутствует, а не отсутствует, что дало бы
  fallback `Ask`;
- `set_all_modes` для `Allow`, `Deny` и `Ask` не меняет `MicrophoneListen`;
- `permissions.json` с правилом `microphone_listen` разбирается без отката на
  дефолты, и `resolve` возвращает `Deny` для любого субъекта;
- allow-list фасада: сериализация каждого варианта ambient-события содержит
  только поля из allow-list и ни одного поля со свободным текстом.

## Критерии готовности

- контракт не имеет побочных эффектов и покрыт негативными тестами;
- capability микрофона существует, по умолчанию запрещена и видна в policy
  snapshot;
- ни одно поле контракта не позволяет протащить в лог содержимое речи;
- новый крейт реально прогоняется в CI, а не проходит мимо списка пакетов.
