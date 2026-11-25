# План 201.3: Прямое чтение состояния Улья в Electron UI из `evohime-hive-daemon`

> **Зависимости:** [201-2-core-hive-module-decoupling.md](201-2-core-hive-module-decoupling.md) (blocking). Optional: нет.

---

## 1. Цель этапа

Полностью вывести `evohime-core` из контура отображения состояния Улья: Electron UI должен получать `HiveStateSnapshot` и `HiveObservationSnapshot` напрямую от `evohime-hive-daemon` через `hive-daemon-client.ts`, по аналогии с `city_map` и `honeycomb`.

## 2. Изменяемые контракты, IPC и UI

1. **Клиент `desktop/evohime-electron/src/main/hive-daemon-client.ts`:**
   - Добавить методы `getHiveState()` (отправляет `HiveObservationControlRequest::GetHiveStateSnapshot`) и `getObservationStatus()` (отправляет `HiveObservationControlRequest::GetObservationStatus`).
   - При недоступности демона возвращать безопасный fallback (`{ available: false, snapshot: null }`), как в `getCityMap` и `getHoneycombWorld`.
2. **IPC-мост `desktop/evohime-electron/src/shared/api.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`:**
   - Добавить прямые команды `hive.getState` и `hive.getObservationStatus`, маршрутизируемые в `options.hiveDaemon`.
   - Сохранить существующие команды настроек (`core.setEvaWorkspacePath`, `core.setHiveSetting` и т.д.) через Core только до тех пор, пока `HiveSettings` хранятся в БД Core через `HiveJournalProvider`.
3. **Renderer `desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx`:**
   - Перевести опрос состояния Улья и статуса наблюдения с `core.getHiveState` / `core.getHiveObservationStatus` (через `CoreEvent`) на прямой ответ `hive.getState` / `hive.getObservationStatus`.
4. **Очистка `evohime-core`:**
   - Удалить прокси-команды `HiveState` и `HiveObservationStatus` из `crates/evohime-core/src/ipc_bridge_hive_state.rs` и `ipc_bridge_ambient_tasks_children.rs` (либо пометить deprecated в `evohime.desktop.proto` до следующего мажорного обновления протокола, не вызывая их из UI).

## 3. Recovery, Verification и Rollback

- **Recovery:** если `evohime-hive-daemon` перезапускается, `OperationsPanel` получает `available: false` и продолжает работу без блокировки основного чата и Core.
- **Verification:**
  - `npm run typecheck`, `npm test` (включая `hive-daemon-client.test.ts`), `npm run check:protocol`, `npm run check:bundle` в `desktop/evohime-electron` (в CI `ui-bundle`).
- **Rollback:** возврат вызова `core.getHiveState` / `core.getHiveObservationStatus` в `OperationsPanel.tsx` и `shell-bridge.ts`.
- **Release evidence:** успешная сборка и тесты `ui-bundle module` и `hive module` в CI.
