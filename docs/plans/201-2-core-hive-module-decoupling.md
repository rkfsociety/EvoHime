# План 201.2: Перенос runtime Улья (реконсиляция, снапшоты, worktree) в `evohime-hive-daemon`

> **Зависимости:** [201-1-core-hive-module-decoupling.md](201-1-core-hive-module-decoupling.md) (blocking). Optional: нет.

---

## 1. Цель этапа

Убрать из процесса `evohime-core` всю исполняемую логику Улья: startup reconciliation дочерних проекций, вычисление `HiveStateSnapshot`, локальный контроллер `HiveObservationHandle::Local` и управление `hive_worktree`.

## 2. Изменяемые контракты и runtime

1. **Startup reconciliation в `evohime-hive-daemon`:**
   - Удалить вызов `reconcile_stale_hive_children_on_startup(&bridge.journal())` из `crates/evohime-core/src/main.rs`.
   - Перенести вызов `evohime_hive::observation::reconcile_stale_hive_children_on_startup` в `crates/evohime-hive/src/bin/daemon.rs`: выполнять идемпотентно при первом успешном подключении демона к `HiveJournalProvider` (с безопасным повтором, если Core стартовал позже демона).
2. **`GetHiveStateSnapshot` на стороне `evohime-hive-daemon`:**
   - В `crates/evohime-hive/src/bin/daemon.rs` обработать `HiveObservationControlRequest::GetHiveStateSnapshot`: вызывать `replay_hive_projections`, `persistent_agent_roles`, `build_hive_observation_for_hive` и `build_hive_state_snapshot`.
   - **Учёт `active_rate_limits`:** в `evohime-core` сейчас используется in-memory `evohime_model_gateway::rate_limit_signal::active_rate_limits()`. Чтобы `evohime-hive-daemon` не зависел от памяти процесса Core, статус rate-limit (или 429 cooldown) должен быть доступен через `HiveJournalProvider` / аудит-события лимитов либо запрашиваться демоном по лёгкому IPC-запросу, а `evohime-hive` уже имеет доступ к `HiveSettings` и `HiveJournalProvider`.
3. **Удаление `HiveObservationHandle::Local` из `evohime-core`:**
   - Удалить `HiveObservationHandle::Local(Arc<HiveObservationController>)` из `crates/evohime-core/src/ipc_bridge_header.rs` и методы `with_observation_daemon` / `with_observation_controller` из `crates/evohime-core/src/ipc_bridge_ambient_transport.rs`.
   - На время до завершения этапа 201.3 `hive_state_snapshot()` в `crates/evohime-core/src/ipc_bridge_hive_state.rs` проксирует запрос `GetHiveStateSnapshot` через `ExternalHiveObservationProxy` (с fallback на `HiveStateSnapshot::default()`, если демон недоступен).
   - Перевести тесты в `crates/evohime-core/src/ipc_bridge_tests_pulse.rs` на лёгкий mock control-pipe сервер вместо `HiveObservationDaemon` и `RoadmapItem`.
4. **Перенос `hive_worktree` в `evohime-hive-daemon`:**
   - В `crates/evohime-hive/src/bin/daemon.rs` перед вызовом `HiveCoordinator::dispatch_task` демон сам вызывает `hive_worktree::prepare_for_task`, передавая готовый `workspace_root` в `HiveTaskDispatchRequest`, и вызывает `hive_worktree::cleanup_if_clean` после завершения задачи (по событию завершения в `HiveJournalProvider`).
   - Удалить вызовы `crate::hive_worktree::prepare_for_task` и `crate::hive_worktree::cleanup_if_clean` из `crates/evohime-core/src/core_coordinator_routing.rs`. Для `evohime-core` задача Улья выполняется просто в переданном `workspace_root`.
   - Если в `core_coordinator_routing.rs` приходят ручные задачи с `route = "hive"` не из `evohime-hive-daemon` (например, из чата/CLI), они не используют `hive_worktree`, а работают в стандартном `chat_worktree` или переданном workspace.

## 3. Recovery, Verification и Rollback

- **Recovery:**
  - Если `evohime-hive-daemon` ещё не успел подключиться к Core при старте, startup reconciliation откладывается до первого успешного `HiveJournalProvider`-запроса и не роняет демон.
  - При перезапуске демона незавершённые worktree-лизы отслеживаются через `HiveJournalProvider` (`has_active_workflow_run` / `mark_worktree_cleanup_pending`).
- **Verification:**
  - Unit-тесты `evohime-hive` на `reconcile_stale_hive_children_on_startup`, `GetHiveStateSnapshot` и `worktree` lifecycle в демоне.
  - Тесты `evohime-core` (`ipc_bridge_tests_pulse.rs`, `core_coordinator_routing`) без использования `evohime_hive`.
- **Rollback:** возврат прокси/вызовов в `evohime-core`; схема БД не меняется.
- **Release evidence:** подтверждённые прогоны `core module` и `hive module` в CI.
