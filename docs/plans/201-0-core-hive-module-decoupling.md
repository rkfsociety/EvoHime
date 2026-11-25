# План 201: Полноценное разделение модулей `evohime-core` и `evohime-hive`

> **Статус:** `active` (этапы `201.1–201.4` готовы к реализации).
> **Границы систем:** Ядро (`evohime-core`) и Улей (`evohime-hive`, `evohime-hive-daemon`) — независимые модули с отдельными бинарниками, релизами (`core.txt` / `hive.txt`) и CI workflow (`core.yml` / `hive.yml`). Единственная допустимая общая точка — минимальный контракт `evohime-hive-contract`.

---

## 1. Проблема и цель

Сейчас процессы `evohime-core.exe` и `evohime-hive-daemon.exe` разделены на уровне `evohime-supervisor.exe` и IPC, но разделение на уровне Rust-крейтов не завершено:

1. `crates/evohime-core/Cargo.toml` содержит прямую зависимость `evohime-hive = { path = "../evohime-hive" }`.
2. `crates/evohime-core/src/lib.rs` реэкспортирует 7 внутренних модулей `evohime_hive`: `pulse`, `pulse_boundary`, `observation`, `settings`, `state`, `permission_session`, `worktree`.
3. Внутри процесса `evohime-core` всё ещё выполняются задачи Улья:
   - startup reconciliation (`reconcile_stale_hive_children_on_startup` в `crates/evohime-core/src/main.rs`);
   - расчёт `HiveStateSnapshot` (`ipc_bridge_hive_state.rs`);
   - локальный контроллер наблюдения `HiveObservationHandle::Local` (`ipc_bridge_header.rs`, `ipc_bridge_ambient_transport.rs`);
   - управление git worktree Улья (`core_coordinator_routing.rs` -> `hive_worktree`);
   - политика разрешений сессии Улья (`core_agent_execution.rs` -> `hive_permission_session::modes()`).
4. Из-за этого доработки Улья периодически требуют правок в `crates/evohime-core` и поднятия `release-versions/core.txt`, что запускает пересборку и публикацию основного ядра.

**Цель плана:** полностью удалить зависимость `evohime-core -> evohime-hive` в `Cargo.toml`, перенести весь runtime Улья в `evohime-hive-daemon`, перевести UI на прямое чтение состояния Улья из демона и защитить границу автоматической проверкой в CI.

---

## 2. Целевая архитектура

```text
Electron UI / Main
  ├── desktop-ipc (named pipe) ─────────────► evohime-core (задачи, модели, инструменты, журнал)
  └── hive-daemon-client (control pipe) ────► evohime-hive-daemon (наблюдение, Honeycomb, CityMap,
                                                    │              HiveState, worktree, реконсиляция)
                                                    ▼
                                            evohime-hive-contract (единственный общий крейт)
```

---

## 3. Зависимости и порядок этапов

- **Blocking dependencies:** отсутствуют (все изменяемые контуры находятся в текущем репозитории).
- **Optional dependencies:** пересечений с планами `199` и `200` нет (план `201` не меняет геометрию/симуляцию Honeycomb, а лишь выносит границы модулей).

| Этап | Файл | Фокус |
| --- | --- | --- |
| 201.1 | [201-1-core-hive-module-decoupling.md](201-1-core-hive-module-decoupling.md) | Вынос DTO, констант границ и control-протокола в `evohime-hive-contract` |
| 201.2 | [201-2-core-hive-module-decoupling.md](201-2-core-hive-module-decoupling.md) | Перенос реконсиляции, расчёта `HiveStateSnapshot` и `hive_worktree` в `evohime-hive-daemon`, удаление `HiveObservationHandle::Local` |
| 201.3 | [201-3-core-hive-module-decoupling.md](201-3-core-hive-module-decoupling.md) | Прямое чтение `hive_state` и `observation_status` из `evohime-hive-daemon` в Electron UI |
| 201.4 | [201-4-core-hive-module-decoupling.md](201-4-core-hive-module-decoupling.md) | Удаление `evohime-hive` из `evohime-core/Cargo.toml`, guard-тест в CI, обновление `core.yml` и закрытие плана |

Этапы выполняются строго последовательно: `201.1 → 201.2 → 201.3 → 201.4`.
