# План 203.3 — Волна 3: Тяжёлые подсистемы (`rag-memory`, `remote-a2a`, `workflow-engine`)

## Цель этапа
Вынести из `evohime-core` три самые объёмные прикладные подсистемы (>1.2 МБ суммарного исходного кода и десятки тест-кейсов).

## Состав переноса и удаления из `evohime-core`
1. **`rag-memory` → `crates/evohime-rag-memory` (`evohime-rag-memory.exe`):**
   - Перенос `workspace_rag.rs`, `workspace_rag_evidence.rs`, `workspace_rag_index.rs`, `workspace_rag_retrieval.rs`, `workspace_rag_tests.rs`, `knowledge_source_registry_project_role.rs`, `project_knowledge_notebook.rs`, `memory_extraction.rs` (74.7 KB), `memory_extraction_tests.rs`, `offline_experience_consolidation.rs`, `experience_replay_library.rs`, `memory_retrieval.rs`.
   - Переключение `core_agent_memory.rs` и `ipc_bridge_ambient_workspace_memory.rs` на `ModuleRegistry` (`rag.search`, `memory.extract` и др.); чтение/запись в SQLite выполняется через чёткий persistence-порт ядра.
2. **`remote-a2a` → `crates/evohime-remote` (`evohime-remote.exe`):**
   - Перенос `external_agent_a2a.rs`, `external_agent_a2a_runtime.rs`, `external_coding_agent_adapter.rs`, `remote_conversation_channels.rs`, `remote_client_control_plane.rs`, `durable_remote_task_bridge.rs`.
   - Переключение `ipc_bridge_advanced_extensions.rs`, `ipc_bridge_workspace_commands_extensions.rs` и `core_coordinator_orchestration_runtime.rs` на `ModuleRegistry`.
3. **`workflow-engine` → `crates/evohime-workflow` (`evohime-workflow.exe`):**
   - Перенос логики и тестов из `workflow.rs` (76.8 KB), `workflow_runtime.rs` (75.6 KB), `workflow_runtime_tests.rs`, `workflow_execution.rs`, `workflow_runner.rs`, `workflow_registry.rs`, `workflow_adapters.rs`, `workflow_package.rs`, `workflow_templates.rs`, `workflow_optimization_lab.rs`, `child_workflow.rs`, `conversational_workflow_composer.rs`, `visual_workflow_builder.rs`, `domain_workflow_recipes.rs`, `automation.rs`, `automation_runtime.rs`, `automation_scheduler.rs`, `automation_simulation.rs`, `automation_acceptance.rs`, `event_trigger_runtime.rs`, `durable_background_execution.rs`, `durable_background_execution_runtime.rs`, `dependency_aware_task_graph.rs`, а также разгрузка `core_coordinator_workflow*.rs`.
   - Оставление в `evohime-core` только тонкого IPC-адаптера для UI и permission checks, делегирующего выполнение в `evohime-workflow.exe`.

## Проверка этапа
- Зелёные CI-пайплайны `rag-memory.yml`, `remote-a2a.yml`, `workflow-engine.yml`, `core.yml`.
- Контрактные тесты ядра на деградацию при временной остановке `workflow-engine` / `rag-memory` (базовый чат и локальные инструменты продолжают работать).
