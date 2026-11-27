# План 202.3 — Вынос тяжёлых модулей и hot-restart в `supervisor` / `updater`

## Цель этапа
Вынести оставшиеся крупные прикладные подсистемы из `evohime-core` и реализовать точечный перезапуск сервисов без остановки UI и основного ядра.

## Области переноса
1. **`remote-a2a`** (на базе `crates/evohime-remote` → `evohime-remote.exe`): `external_agent_a2a*`, `external_coding_agent_adapter`, `remote_conversation_channels`, `remote_client_control_plane`, `durable_remote_task_bridge`.
2. **`ambient`** (`crates/evohime-ambient` → `evohime-ambient.exe`): `ambient.rs`, `ambient_proactivity.rs`, `autonomous_metric_experiment_runtime.rs`, `temporal_signal_intelligence.rs`.
3. **`computer-use`** (`crates/evohime-computer-use` → `evohime-computer-use.exe`): `native_computer_use_runtime`, `mobile_device_automation_runtime`, `ide_companion_bridge`, `cross_modal_ui_grounding` (с переносом Win32-GUI зависимостей `windows` из ядра в этот модуль).
4. **`workflow-engine`** (`crates/evohime-workflow` → `evohime-workflow.exe`): `workflow*.rs`, `automation*.rs`, `event_trigger_runtime.rs`, `durable_background_execution*.rs`, `dependency_aware_task_graph.rs`.
5. **`rag-memory`** (`crates/evohime-rag-memory` → `evohime-rag-memory.exe`): `workspace_rag*`, `knowledge_source_registry_project_role.rs`, `project_knowledge_notebook.rs`, `memory_extraction*`, `offline_experience_consolidation.rs`, `experience_replay_library.rs`.

## Изменяемые контракты (IPC, `supervisor`, `updater`)
- `evohime-supervisor`: управление пулом зарегистрированных сервисов под Job Object, независимый старт/стоп/рестарт конкретного сервиса по команде от `updater`.
- `evohime-updater` и `evohime-transaction`: поддержка `restart = "service:<id>"` — обновление заменяет `.exe` конкретного модуля и перезапускает только его, не прерывая работу `EvoHime.exe` и `evohime-core.exe`.
- IPC-проекции в UI для отображения состояния подключённых модулей (ready / updating / unavailable).

## Зависимости
- Блокирующие: Этапы 202.1 и 202.2.
- Опциональная: План 194 (отображение фоновых задач `workflow-engine` и `ambient`).

## Recovery и rollback
- При сбое обновления конкретного модуля `evohime-transaction` откатывает только его бинарник, не затрагивая остальные модули.
- Если тяжёлый модуль (`rag-memory` или `workflow-engine`) временно недоступен, ядро продолжает обслуживать базовый чат и локальные инструменты.

## Проверка
- Интеграционные тесты `supervisor` на запуск, изоляцию падения и точечный рестарт одного сервиса.
- E2E-тест `evohime-updater` на обновление одного прикладного модуля без перезапуска `evohime-core.exe`.
