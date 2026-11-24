# План 200.2 — Runtime-интеграция маршрутизатора Королевы и `delivery_requirements`

Статус: planned. Обзор: [`200-0-hive-queen-router-and-dynamic-agents.md`](200-0-hive-queen-router-and-dynamic-agents.md).

## Scope и изменяемые контракты

1. **Ограничение прямых инструментов Королевы (`crates/evohime-core/src/core_agent_execution.rs`):**
   - Для `hive_world && agent_depth == 0` исключить из `specs` прямые инструменты `filesystem_*`, `git_*`, `shell_execute`, `browser_*`.
   - Оставить Королеве только делегирование (`hive_delegate_agent`), постановку/проверку целей (`goal_create`, `goal_verify`), анализ/сводки и регистрацию ролей (`hive_register_role`).
2. **Адаптация `delivery_requirements` под делегированные отчёты (`crates/evohime-core/src/core_agent_execution.rs`):**
   - Не полагаться на `degrade_unavailable(&tool_names)` для Королевы (иначе она сможет закрыть задачу без реальных правок/проверок).
   - Закрывать `research_done`, `mutation_done`, `verification_done`, `commit_done` у Королевы на основании принятых `TypedChildReport` от соответствующих ролей (`Researcher`, `Implementer`, `Tester`, `Reviewer`).
3. **Промпты маршрутизатора (`crates/evohime-core/src/core_prelude.rs`, `crates/evohime-hive/src/pulse.rs`):**
   - Обновить `proactive_evolution_prompt`, `build_recovery_prompt` и системный промпт Королевы: закрепить цикл `researcher → planner → подтверждение плана пользователем → implementer → tester → reviewer`.

## Зависимости

### Блокирующие
- Завершение [`200-1-hive-queen-router-and-dynamic-agents.md`](200-1-hive-queen-router-and-dynamic-agents.md).

### Опциональные
- Отсутствуют.

## Recovery и rollback

- При отказе или ошибке отчёта дочернего агента Королева перенаправляет задачу обратно `Implementer` или фиксирует диагностику через `TypedChildReport`.
- Rollback: возврат прямых инструментов в `specs` Королевы флагом маршрутизации.

## Verification

- Unit-тесты `core_agent_execution`: `specs` Королевы не содержит `filesystem_*`/`git_*`/`shell_execute`, а `delivery_requirements` закрывается отчётами ролей.

## Release evidence

- Логи `model-trace.jsonl` и `audit.jsonl` в тестовом прогоне: Королева вызывает только делегирование, а не прямые файловые инструменты.
