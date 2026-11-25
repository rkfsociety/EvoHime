# План 201.1: Контрактные DTO, границы задач и протокол в `evohime-hive-contract`

> **Зависимости:** [201-0-core-hive-module-decoupling.md](201-0-core-hive-module-decoupling.md). Blocking: нет. Optional: нет.

---

## 1. Цель этапа

Убрать типовую зависимость `evohime-core` от `evohime-hive`, перенеся в `evohime-hive-contract` только чистые DTO, константы границ и типы IPC control-протокола — **без** добавления в `evohime-hive-contract` зависимостей от `evohime-model-gateway` или `evohime-permissions`.

## 2. Изменяемые контракты и файлы

1. **`crates/evohime-hive-contract/Cargo.toml`:**
   - Сохранить минимальный набор зависимостей (`serde`, `serde_json`, `thiserror`), без `evohime-model-gateway` и `evohime-permissions`.
2. **Новые модули в `crates/evohime-hive-contract/src/`:**
   - `pulse_boundary`: `EVA_PULSE_TASK_PREFIX`, `parse_hive_task_uuid`.
   - `settings`: `HiveSettings`, `HiveSettingsError`, `HiveProviderRouteSettings`, `HiveProviderLimits` (чистый DTO с `From`/`Into` конверсией к `evohime_model_gateway::ProviderLimits` на стороне `evohime-core` и `evohime-hive`), `DelegationComplexity`, `HIVE_SETTINGS_SCOPE`, `MAX_SUBAGENT_HARD_LIMIT`, `MAX_DAILY_BUDGET_USD`, `validate_eva_workspace_path`, `paths_refer_to_same_repo`.
   - `state`: `HiveStateSnapshot` (как сериализуемый DTO без конструктора `from_observation`, зависящего от `observation.rs`), `HIVE_DAY_WINDOW_MS`, `quota_window_start_ms`, константы `HIVE_OBSERVATION_*` и `HIVE_QUEEN_*`.
   - `control`: `HiveObservationControlRequest` (включая новый вариант `GetHiveStateSnapshot`), `HiveObservationSnapshot`, `DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE`, `HIVE_OBSERVATION_CONTROL_PIPE_ENV`, `HIVE_OBSERVATION_CONTROL_SOCKET_ENV`.
3. **Обратная совместимость в `crates/evohime-hive`:**
   - Реэкспортировать перенесённые типы из `evohime_hive_contract` в `crates/evohime-hive/src/{pulse_boundary,settings,state,pulse}.rs`, чтобы не ломать внутренние импорты `evohime-hive`.
   - Конструктор `HiveStateSnapshot` из `HiveObservation` (`build_hive_state_snapshot`) оставить внутри `evohime-hive` (`state.rs` / `observation.rs`).
4. **Политика разрешений Улья (`hive_permission_session::modes`):**
   - `evohime-permissions` — крейт ядра. В `crates/evohime-core/src/core_agent_execution.rs` перенести 8-строчный профиль read-allow / write-ask напрямую в локальный helper `hive_world_permission_modes()` (или в `evohime-permissions`), убрав вызов `crate::hive_permission_session::modes()`.
5. **Переключение импортов в `crates/evohime-core/src/`:**
   - Перевести `child_contracts.rs`, `child_executor.rs`, `core_agent_execution.rs`, `core_journal_children.rs`, `free_provider_reliability_routing.rs`, `hive_context.rs`, `lazy_delegation.rs`, `workflow_adapters.rs`, `external_pulse.rs`, `main.rs` на `evohime_hive_contract::*`.

## 3. Recovery, Verification и Rollback

- **Recovery:** обратная совместимость JSON-схем `HiveSettings` и `HiveStateSnapshot` сохраняется (`#[serde(default)]`, прежние имена полей и `HIVE_SETTINGS_SCHEMA_VERSION` / `HIVE_STATE_SCHEMA_VERSION`).
- **Verification:**
  - `cargo test -p evohime-hive-contract`
  - Проверка компиляции и тестов `evohime-hive` и `evohime-core` в CI (или узкий `cargo test` по затронутым модулям).
- **Rollback:** откат коммита этапа 201.1; внешние форматы БД и IPC-фреймов не меняются.
- **Release evidence:** на переходном этапе (если коммиты 201.1–201.2 выходят вместе) обновляются `release-versions/core.txt` и `release-versions/hive.txt`.
