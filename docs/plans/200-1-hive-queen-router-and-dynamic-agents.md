# План 200.1 — Core-контракт ролевого делегирования, прав и ролей

Статус: planned. Обзор: [`200-0-hive-queen-router-and-dynamic-agents.md`](200-0-hive-queen-router-and-dynamic-agents.md).

## Scope и изменяемые контракты

1. **Расширение `ChildRole` (`crates/evohime-core/src/child_roles.rs`):**
   - Добавить `git.write` в допустимые capabilities для роли `Implementer` (или выделенной релизной роли), чтобы агенты могли завершать цикл коммитом в worktree.
   - Для `ChildRole::Custom` разрешить привязку capability-профиля из `crates/evohime-core/src/agent_role_profiles.rs` вместо жёсткого `["workspace.read", "workspace.search"]`.
2. **Ролевой инструмент делегирования Королевы (`hive_delegate_agent`):**
   - Обёртка поверх `TypedChildTaskRequest` (`role`, `department`, `prompt`, `scope`, `acceptance_criteria`, `grants`, `plan_summary`).
   - Обязательное `plan_summary` при делегировании мутирующих/проверочных ролей (`Implementer`, `Tester`) до подтверждения плана — передаётся в `ApprovalRequired.preview`.
3. **Разделение `read_only_subagent` и ролевых воркеров (`crates/evohime-core/src/core_agent_execution.rs`):**
   - Заменить жёсткий `read_only_subagent` для дочерних задач Улья на `DelegationGrant` + `ChildRole` в контексте per-task `session_id` задачи при сохранении `MAX_CHILD_DEPTH = 2` и `grants ⊆ parent_grants`.
   - Подагенты `depth = 2` остаются без права дальнейшего делегирования и внутри выданных `grants`.

## Зависимости

### Блокирующие
- Принятый обзор [`200-0-hive-queen-router-and-dynamic-agents.md`](200-0-hive-queen-router-and-dynamic-agents.md).

### Опциональные
- Отсутствуют.

## Recovery и rollback

- При перезапуске дочерние задачи восстанавливаются по существующему `coordinator_child_checkpoint` и `lease` из [`../architecture/hive.md`](../architecture/hive.md).
- Rollback: отключение `hive_delegate_agent` возвращает старый read-only `agent_run` без изменения схемы БД.

## Verification

- Unit-тесты в `evohime-core`: проверка `allowed_capabilities`, привязки `Custom` к `agent_role_profiles`, наследования `session_id` мутирующими ролями и отклонения `depth > 2` / расширения `grants`.

## Release evidence

- Тестовый прогон `evohime-core` (в CI), проверка `git diff --check`.
