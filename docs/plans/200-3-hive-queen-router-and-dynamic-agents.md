# План 200.3 — Динамические сектора и синхронизация 3D-машин с событиями `child.*`

Статус: planned. Обзор: [`200-0-hive-queen-router-and-dynamic-agents.md`](200-0-hive-queen-router-and-dynamic-agents.md).

## Scope и изменяемые контракты

1. **Регистрация новых ролей и секторов (`crates/evohime-core/src/agent_role_profiles.rs`, `crates/evohime-hive/src/state.rs`, `crates/evohime-hive/src/bin/daemon.rs`):**
   - Добавить операцию/инструмент `hive_register_role` (для агента-архитектора/Королевы), сохраняющий новую роль (название, сектор, специализация, bounded capabilities) в `events.db`.
   - Проецировать созданные роли в `HiveStateSnapshot::departments` и в генератор `HiveCityMapSnapshot` в `evohime-hive-daemon`, чтобы новые сектора/дома появлялись на 3D-карте автоматически.
2. **Синхронизация 3D-машин с жизненным циклом `child.*` (`desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx`, `HiveCityScene.tsx`, `hive-agent-simulation.tsx`):**
   - Удерживать `queenWorking` / `queenActive` в `true` на протяжении всей активной задачи Улья (включая время работы дочерних агентов, пока нет паузы или ожидания approval).
   - Связать фазы машины (`delivering` → работа дома → `returning`) с реальными событиями `child.requested` / `Running` и `TypedChildReport`.

## Зависимости

### Блокирующие
- Завершение [`200-2-hive-queen-router-and-dynamic-agents.md`](200-2-hive-queen-router-and-dynamic-agents.md).

### Опциональные
- Расширения карты-архипелага из [`199-3-hive-living-architecture-archipelago.md`](199-3-hive-living-architecture-archipelago.md).

## Recovery и rollback

- Зарегистрированные роли хранятся в SQLite и восстанавливаются при старте Core/демона.
- Rollback: откат проекции UI к статическому набору департаментов без потери данных в БД.

## Verification

- Тесты `evohime-hive` (`state.rs`: динамические роли в `departments` и `HiveCityMapSnapshot`) и тесты Electron (`OperationsPanel` / `HiveCityScene` / `hive-studio`).

## Release evidence

- Снимок `HiveStateSnapshot` и визуальная проверка в `hive-studio` / Electron UI.
