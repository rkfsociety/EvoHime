# План 199.2 — Интеграция в `HiveObservationDaemon` и control pipe `evohime-hive-daemon`

Статус: active implementation contract. Этап следует после 199.1.

## Scope и изменяемые контракты
1. Добавить кэш карты архипелага внутри `HiveObservationDaemon` (`crates/evohime-hive/src/pulse.rs`) или связанного состояния демона (`crates/evohime-hive/src/bin/daemon.rs`):
   - Хранить актуальный `Arc<RwLock<HiveCityMapSnapshot>>` и bounded-рекурсивную сигнатуру кэша по каждому репозиторию: список путей + `(relative_path, entry_count, mtime)` для директорий в пределах глубины сканирования (до 6 уровней, `≤ MAX_ENTRIES_PER_DIR`) + `(relative_path, size, mtime)` ключевых манифестов (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `.sln`/`.csproj`). Правки содержимого обычных исходников без изменения списка файлов и манифестов подхватываются на плановом полном пересканировании (`line_count` — приблизительная метрика).
   - **Коалесинг пересчётов и параллелизм:** одновременно выполняется не более **одной** задачи пересчёта карты в `tokio::task::spawn_blocking` (guard через `AtomicBool` / флаг `rescan_requested`). Повторный триггер во время активного скана не запускает второй параллельный `spawn_blocking`, а планирует ровно один повторный проход после завершения текущего.
   - На каждом Level 1 цикле локального наблюдения (`local_observation_interval_secs`, без LLM) проверять изменение `eva_workspace_path`, `watched_repositories` и сигнатур; пересканировать только изменившиеся репозитории. При попадании в кэш возвращать кэшированный `HiveCityMapSnapshot` целиком (со старым `generated_at_ms` и неизменным `revision_hash`).
   - **Eventual consistency после `NotifySettingsChanged`:** при получении `NotifySettingsChanged` демон немедленно ставит пересчёт в фоновую очередь `spawn_blocking`; пока скан идёт, `GetCityMap` без блокировки отдаёт последний валидный снимок, а UI (в 199.3) делает короткий burst-polling после сохранения настроек до смены `revision_hash`.
2. Расширить собственный control IPC протокол Улья (`crates/evohime-hive/src/pulse.rs` и `crates/evohime-hive/src/bin/daemon.rs`):
   - Добавить аддитивный вариант `HiveObservationControlRequest::GetCityMap`.
   - В `handle_observation_control_connection` обрабатывать `GetCityMap` возвратом сериализованного `HiveCityMapSnapshot` (размером `≤ MAX_CITY_MAP_PAYLOAD_BYTES = 256 KiB`) в том же 4-byte LE length-prefixed JSON frame (`evohime_desktop_ipc::transport::write_frame`).
   - **Изоляция медленных клиентов и таймауты:** сохранить существующую независимую обработку каждого входящего соединения через `tokio::spawn` в `run_observation_control_server` (`crates/evohime-hive/src/bin/daemon.rs`) и добавить явные `read_timeout = 1000 ms` и `write_timeout = 1500 ms` (согласовано с клиентским таймаутом `1500 ms` в 199.3), чтобы медленный читатель `GetCityMap` не держал сокет и не влиял на `GetObservationStatus`.
   - **Разграничение ответственности за ошибки протокола:** демон на `GetCityMap` всегда возвращает валидный `HiveCityMapSnapshot` JSON (даже на холодном старте или без настроенных путей — минимум Столицу Евы из `embedded_manifest`, а недоступные спутники с `status = "unavailable"` внутри массива `satellites`). Статус `unavailable` на уровне команды `hive.getCityMap` формируется строго на стороне клиента (`hive-daemon-client.ts` в 199.3) при ошибке подключения к pipe, таймауте или невалидном фрейме.
   - Сохранить 100% обратную совместимость ответов на существующие запросы `GetObservationStatus` и `NotifySettingsChanged` (формат `HiveObservationSnapshot` не меняется, поэтому `ExternalHiveObservationProxy` в `evohime-core` продолжает работать без единой правки).
3. Поддерживать изоляцию процесса Улья:
   - `evohime-core` не получает новых методов, прокси или proto-сообщений.
   - `evohime-hive-daemon` продолжает работать под тем же owner-only DACL на `\\.\pipe\evohime-hive-observation` (Windows) и `hive-observation.sock` (Unix).

## Зависимости
### Блокирующие
- Этап 199.1 (`city_map` модуль и контракт `HiveCityMapSnapshot`).
- Существующий control pipe сервер `run_observation_control_server` в `crates/evohime-hive/src/bin/daemon.rs`.

### Опциональные
- Нет.

## Recovery и rollback
- Если фоновое сканирование не успело завершиться на старте демона, `GetCityMap` сразу отдаёт валидный начальный снимок (как минимум Столицу Евы из `embedded_manifest` и пустой/кэшированный список спутников), не блокируя соединение.
- Ошибка `spawn_blocking` или I/O-ошибка при сканировании логируется через `tracing::warn`, не переводит `HiveObservationDaemon` в `sleeping` (не нарушает основной цикл наблюдения) и сохраняет предыдущий валидный снимок с диагностической пометкой недоступного острова.
- Rollback этапа: удаление `GetCityMap` и кэша карты из `pulse.rs` / `daemon.rs`; `evohime-core` не затрагивается.

## Verification
- Интеграционные тесты control pipe в `crates/evohime-hive`:
  - `GetCityMap` возвращает валидный `HiveCityMapSnapshot` с корректным 4-byte LE framing;
  - `GetObservationStatus` и `NotifySettingsChanged` сохраняют прежний JSON-контракт без регрессий;
  - добавление и удаление временного репозитория в настройках обновляет `satellites` и меняет `revision_hash`;
  - повторный цикл без изменений на диске попадает в кэш и не перестраивает снимок.

## Release evidence
- Логи и результаты интеграционных тестов control pipe `evohime-hive-daemon`.
- Подтверждение отсутствия diff в `crates/evohime-core/` и `crates/desktop-ipc/proto/`.

## Критерии выхода
- [ ] `evohime-hive-daemon` отдаёт `HiveCityMapSnapshot` по `GetCityMap` напрямую по своему control pipe.
- [ ] Изменение `watched_repositories` приводит к быстрому обновлению карты через Level 1 / `NotifySettingsChanged`.
- [ ] `GetObservationStatus` и `NotifySettingsChanged` полностью обратно совместимы.
- [ ] `evohime-core` не изменён.
