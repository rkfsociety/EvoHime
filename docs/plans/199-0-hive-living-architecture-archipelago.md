# План 199.0 — Живой город-архипелаг реальной архитектуры Улья

Статус: active implementation contract. Вся логика карты архитектуры принадлежит независимому модулю `evohime-hive` (`evohime-hive-daemon`) и отображается в Electron UI напрямую через control pipe Улья; `evohime-core` и `desktop-ipc` proto не затрагиваются.

## Цель
Заменить статичную панораму с абстрактными департаментами в [HiveCityScene.tsx](../../desktop/evohime-electron/src/renderer/src/components/3d/HiveCityScene.tsx) на **живой кликабельный изометрический архипелаг городов без вращения камеры**:
- в центре — **Столица Евы**, отражающая реальную текущую архитектуру EvoHime (crates, оболочка, домены и зависимости);
- вокруг неё — **города-спутники** для каждого проекта из списка `watched_repositories`: при добавлении нового проекта в наблюдение рядом автоматически «строится» его город, отражающий реальную структуру этого репозитория, и соединяется со Столицей световым мостом; при удалении проекта город сворачивается;
- город остаётся «живым» (пульс наблюдения, потоки света по дорогам, дроны активных агентов Улья, кликабельный инспектор зданий и связей, плавный фокус/зум на выбранный город без вращения угла камеры).

## Основание checkout
- `evohime-hive-daemon` ([daemon.rs](../../crates/evohime-hive/src/bin/daemon.rs)) уже работает как отдельный процесс в собственном Job Object ([windows_supervisor.rs](../../crates/evohime-supervisor/src/windows_supervisor.rs)) и публикуется отдельным модулем `release-versions/hive.txt`.
- У `evohime-hive-daemon` уже есть собственный owner-only control named pipe (`\\.\pipe\evohime-hive-observation`, на Unix — `hive-observation.sock`) с 4-byte little-endian length-prefixed JSON framing (`HiveObservationControlRequest` в [pulse.rs](../../crates/evohime-hive/src/pulse.rs)) и Level 1 цикл локального наблюдения без LLM.
- В [desktop/evohime-electron/package.json](../../desktop/evohime-electron/package.json) уже установлены `three` (`0.187.0`) и `@react-three/fiber` (`9.8.2`); новых production-зависимостей не требуется.
- Preload ([src/preload/index.ts](../../desktop/evohime-electron/src/preload/index.ts)) проксирует `api.invoke` по `RendererCommand` из `@shared/api` в `shell-bridge.ts`, что позволяет добавить прямой канал `hive.getCityMap` без расширения preload-поверхности и без участия Core.

## Граница
- **Строгая независимость от Core:** `evohime-core` и `crates/desktop-ipc/proto/evohime.desktop.proto` не изменяются; `release-versions/core.txt` не затрагивается. Core не сканирует репозитории для карты городов и не проксирует `GetCityMap`.
- **Единственный владелец карты мира — `evohime-hive`:** сканирование `eva_workspace_path` и `watched_repositories`, кэширование, санитизация путей и формирование `HiveCityMapSnapshot` выполняются только внутри `evohime-hive-daemon` (Level 1, read-only, без LLM и без запуска сборки/команд).
- **Безопасность ФС:** канонизация путей, запрет path traversal и symlink escape за пределы корня репозитория, пропуск `.git`, `target`, `node_modules`, `.evohime`, `dist`, `out`, `build`, жёсткие лимиты на глубину, число файлов, узлов, рёбер и размер сериализованного снимка.
- **Renderer остаётся read-only проекцией:** UI не обращается к ФС напрямую, не читает репозитории и не хранит авторитетное состояние; Electron main читает только `HiveCityMapSnapshot` по owner-only pipe `evohime-hive-daemon`.
- **Камера без вращения:** изометрический ракурс фиксирован (без `OrbitControls` вращения); допускаются только клики по зданиям/городам, подсветка связей и плавный перевод фокуса/зум на выбранный остров с возвратом к общему виду.

## Изменяемые контракты
- `evohime-hive`: новый модуль `city_map` (`HiveCityMapSnapshot`, `CityIsland`, `CityDistrict`, `CityBuilding`, `CityEdge`, версия схемы `HIVE_CITY_MAP_SCHEMA_VERSION = 1`, `deny_unknown_fields`). Любое новое поле требует инкремента `HIVE_CITY_MAP_SCHEMA_VERSION`.
- `evohime-hive::pulse`: аддитивный вариант `HiveObservationControlRequest::GetCityMap` на собственном pipe Улья (полная обратная совместимость с `GetObservationStatus` и `NotifySettingsChanged`).
- `evohime-electron`: новый main-клиент `hive-daemon-client.ts` для прямого подключения к `\\.\pipe\evohime-hive-observation` / `hive-observation.sock`, команда `hive.getCityMap` в `@shared/api` и `shell-bridge.ts`, переработка [HiveCityScene.tsx](../../desktop/evohime-electron/src/renderer/src/components/3d/HiveCityScene.tsx) и её интеграция в [OperationsPanel.tsx](../../desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx).

### Сводная таблица уровней лимитов фрейма и снимка
| Уровень | Константа | Лимит | Поведение при превышении |
| --- | --- | --- | --- |
| Серверный бюджет снимка (`evohime-hive`) | `MAX_CITY_MAP_PAYLOAD_BYTES` | `256 KiB` | Детерминированное усечение (рёбра → второстепенные здания); крайний fallback — `embedded_manifest` Столицы |
| Клиентский лимит приёма (`evohime-electron`) | `MAX_CITY_MAP_FRAME_BYTES` | `512 KiB` | Отклонение фрейма и возврат типизированного `unavailable` из `hive-daemon-client.ts` |
| Транспортный лимит pipe (`desktop-ipc`) | `MAX_FRAME_BYTES` | `4 MiB` | `FrameError::TooLarge` на уровне транспорта |

## Зависимости
### Блокирующие
- Существующий standalone `evohime-hive-daemon` и его owner-only control pipe (`evohime-hive`, `evohime-desktop-ipc::transport`).
- Существующий механизм синхронизации `eva_workspace_path` и `watched_repositories` в `evohime-hive-daemon` и `NotifySettingsChanged`.
- Существующий Electron main/preload/renderer bridge (`@shared/api`, `shell-bridge.ts`, `OperationsPanel.tsx`) и уже установленные `three` / `@react-three/fiber`.

### Опциональные
- План 194 (Background Jobs Control Center) может в будущем показывать ссылки на узлы/проекты; не блокирует 199.
- План 193 (Agent Coordination) может в будущем обогащать привязку активных агентов Улья к конкретному модулю; на этапе 199 достаточно уже доступных полей активных агентов и репозиториев Улья.

## Recovery и rollback
- Если `eva_workspace_path` не настроен или недоступен, Столица Евы строится из встроенного канонического манифеста архитектуры EvoHime (`source = "embedded_manifest"`); при наличии валидного `eva_workspace_path` используется живое сканирование (`source = "live_workspace"`).
- Если один из `watched_repositories` временно недоступен, удалён или содержит повреждённый манифест, он не ломает остальной архипелаг: его остров получает статус `unavailable` с диагностическим кодом причины, а остальные города обновляются штатно.
- Если `evohime-hive-daemon` не запущен или недоступен по pipe, `hive.getCityMap` возвращает типизированный `unavailable`, а UI показывает аккуратный fallback без ошибок и без влияния на работу Core.
- Если WebGL недоступен или включён `prefers-reduced-motion`, сцена автоматически переключается на лёгкий 2D/статичный режим без потери кликабельности инспектора.
- Rollback выполняется откатом модулей `hive`, `shell-host` и `ui-bundle` без миграций SQLite и без изменений `core`.

## Этапы
- [Этап 1 — Контракт `HiveCityMapSnapshot` и read-only сканер архитектуры в `evohime-hive`](./199-1-hive-living-architecture-archipelago.md)
- [Этап 2 — Интеграция в `HiveObservationDaemon` и control pipe `evohime-hive-daemon`](./199-2-hive-living-architecture-archipelago.md)
- [Этап 3 — Прямой клиент Electron main и живая кликабельная изометрическая сцена архипелага](./199-3-hive-living-architecture-archipelago.md)
- [Этап 4 — Verification, release evidence и закрытие](./199-4-hive-living-architecture-archipelago.md)

## Критерии готовности
- [ ] `evohime-core` и `crates/desktop-ipc/proto/evohime.desktop.proto` не изменены.
- [ ] `evohime-hive` строит `HiveCityMapSnapshot` с детерминированными `revision_hash`, `structure_hash` и сортировкой коллекций (без зависимости от volatile-поля `generated_at_ms`) для Столицы Евы и всех `watched_repositories`.
- [ ] Сканер соблюдает границы безопасности путей, игнорирует служебные каталоги и укладывается в жёсткие лимиты узлов/рёбер/размера payload.
- [ ] Добавление/удаление проекта в `watched_repositories` обновляет архипелаг в `evohime-hive-daemon` и вызывает анимацию строительства/сворачивания города-спутника в UI.
- [ ] Сцена в `HiveCityScene.tsx` живая, кликабельная, без вращения камеры, с инспектором зданий/связей, фокусом на выбранный город и безопасным 2D-fallback.
- [ ] Все unit/integration/security проверки `evohime-hive` и `evohime-electron` подтверждены.

## Non-goals
- Внесение логики карты городов или сканера репозиториев в `evohime-core` либо добавление новых proto-команд в `desktop-ipc`.
- Свободное 3D-вращение камеры (`OrbitControls` rotate), редактирование кода/файлов из сцены города, запуск сборок (`cargo metadata`, `npm`, `git` write) ради построения графа.
- Автоматическое добавление обычных чатовых workspaces в архипелаг (архипелаг отражает только мир Улья: Еву и `watched_repositories`).
- Добавление новых production npm/cargo зависимостей.

## Verification и release evidence
Unit-тесты сканера и лимитов в `evohime-hive` → тесты control pipe `GetCityMap` + обратной совместимости `evohime-hive-daemon` → unit/security-тесты `hive-daemon-client`, `shell-bridge`, `HiveCityScene` и `check:bundle` в `evohime-electron` → CI acceptance и публикация модулей `hive`, `shell-host`, `ui-bundle`.

## Критерии выхода overview
- [ ] Независимость `evohime-hive` и `evohime-core` подтверждена границей контрактов и отсутствием изменений в Core.
- [ ] Зафиксированы bounded-лимиты, fallback-режимы и политика безопасности ФС/IPC.
- [ ] Все четыре этапа согласованы без циклических и обратных зависимостей.
