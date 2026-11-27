# EvoHime — доменный контракт: микроядро Core и изолированные модули-сервисы

Канонический контракт микроядерной декомпозиции Core (план 202, закрыт
2026-11-27). Обновлено: 2026-11-27.
Остов архитектуры: [../architecture.md](../architecture.md).

## Границы микроядра

В `evohime-core.exe` остаётся только то, что обязано быть единым владельцем
состояния и безопасности: главный цикл агента и оркестрация, транспорт
IPC (`desktop-ipc`, `pipe_server`, `ipc_bridge*`), хранилище SQLite и журнал
событий, политики безопасности и разрешения, базовый маршрутизатор вызовов
к моделям и инструментам, а также универсальный реестр внешних модулей-сервисов
(`ModuleRegistry`). Прикладная бизнес-логика живёт в изолированных
модулях-сервисах; фактический перенос оставшейся логики и очистка ядра
выполняются по плану 203.

## Единый контракт `evohime-service-contract`

Все модули-сервисы реализуют один контракт:

- Транспорт: length-prefixed JSON-кадры с ограничением размера
  (`MAX_SERVICE_FRAME_BYTES`) поверх stdin/stdout дочернего процесса,
  который спавнит `evohime-core` (`StdioProcessTransport`).
- Аутентификация: HMAC-SHA256-рукопожатие. Ядро генерирует одноразовый
  секрет сессии (32 байта, hex) и передаёт его дочернему процессу только
  через переменную окружения `EVOHIME_SERVICE_SESSION_SECRET`. Первый
  запрос обязан быть `Authenticate { nonce, proof }`, где `proof` —
  `HMAC-SHA256(secret, nonce)`; nonce — lowercase hex ровно 64 символа,
  сравнение — constant-time. Любой другой запрос до рукопожатия и повторный
  `Authenticate` отклоняются `PermissionDenied` без раскрытия секрета.
  Неудачное рукопожатие убивает дочерний процесс; следующий вызов порождает
  новый с новым секретом. Без переменной окружения (dev/тесты) гейтинг
  выключен.
- Методы: `Ping`, `Health`, `DescribeCapabilities` (имена возможностей,
  JSON Schema, требуемые `permissions`), `Invoke`, `Shutdown`; стриминг
  объявляется флагом `supports_streaming` и идёт инкрементальными
  `StreamChunk`.
- Коды ошибок: `Unavailable`, `Timeout`, `InvalidInput`, `PermissionDenied`,
  `NotFound`, `Internal`. Недоступный модуль не роняет ядро: `ModuleRegistry`
  возвращает typed `Unavailable`.

## Реестр и вызов сервисов

`ModuleRegistry` (`crates/evohime-core/src/module_registry.rs`) автоматически
обнаруживает сервисные экзешники рядом с `evohime-core.exe`
(`SERVICE_MODULE_EXECUTABLES`, 12 модулей), опрашивает их capabilities,
регистрирует их в реестре и проксирует вызовы. Перед `Invoke` ядро проверяет
`permissions`; capability-имена уникальны и принадлежат ровно одному модулю.
Вызовы прикладных подсистем идут через `ModuleRegistry::invoke_if_ready`
с typed `Unavailable` при недоступности сервиса.

## Состав модулей

12 изолированных Rust-сервисов без зависимости от `evohime-core`:
`evals` (`evohime-evals-service`), `media-voice`, `research`,
`remote-a2a` (`evohime-remote`), `ambient`, `computer-use`,
`workflow-engine` (`evohime-workflow`), `rag-memory`, `tools-workspace`,
`tools-exec`, `model-adapters`, `model-router`. Существующие модули
`analysis-worker`, `hive`, `verifier` доукомплектованы переносимыми
подсистемами по плану 203.

## Единый реестр сборки и доставки

`modules.manifest.json` в корне репозитория — канонический список 26 модулей
(`id`, `kind`, `artifact`, `restart`, `version_file`, `workflow`,
`dependencies`). Его согласованность с `module-router.yml`, релизными
маркерами и упаковкой проверяют `scripts/check-modules-manifest.ps1`
(входит в `pre-push-check.ps1`) и `scripts/module-router.tests.ps1`.

## Пакет данных `core-assets`

Системные промпты, шаблоны рецептов и декларативные профили поставляются
отдельным `.zip`-пакетом (`assets/core-assets/`) без компиляции Rust.
Ядро загружает его через `crates/evohime-core/src/core_assets.rs` с проверкой
SHA-256 манифеста; при отсутствии пакета или несовпадении хеша используется
встроенный fallback. Сборка и публикация — `.github/workflows/core-assets.yml`.

## Точечный hot-restart сервиса

Обновление прикладного модуля не прерывает `EvoHime.exe` и `evohime-core.exe`:

1. `evohime-update-agent` определяет изолированные обновления
   (`restart = "service:<id>"`) и перед применением останавливает только
   процессы обновляемых сервисов
   (`evohime_updater::terminate_service_processes`): работающий экзешник
   блокирует свой файл, а сервисы не хранят состояние между вызовами.
2. Транзакция заменяет только `.exe` обновлённых модулей
   (`UpdateTransaction::SELECTABLE_COMPONENTS`).
3. Ядро обнаруживает смерть дочернего процесса и лениво перезапускает сервис
   с новым бинарником при следующем вызове, повторяя HMAC-рукопожатие.
4. `evohime-supervisor` (`service_pool.rs`) отслеживает процессы сервисных
   модулей в тиковом цикле; если бинарник на диске новее, чем момент старта
   процесса (обновление прервано посреди транзакции), supervisor завершает
   именно этот процесс (событие `service_hot_restart` в журнале) — ядро
   перезапускает только его. Белый список имён защищает компоненты ядра,
   оболочки и supervisor от ошибочного завершения.
