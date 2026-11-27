# План 203.2 — Волна 2: `evals` и доперенос в `analysis-worker`, `verifier`, `hive`

## Цель этапа
Вынести из `evohime-core` подсистему оценок (`evals`) и завершить перенос прикладных модулей в уже существующие сервисы `analysis-worker`, `verifier` и `hive`.

## Состав переноса и удаления из `evohime-core`
1. **`evals` → `crates/evohime-evals-service` (`evohime-evals.exe`):**
   - Перенос `evals.rs`, `evals_tests.rs`, `evals_workflow_orchestration.rs`, `execution_trace_evaluation.rs`.
   - Переключение вызовов запуска оценок/бенчмарков из `core` на `ModuleRegistry` (`evals.*`).
2. **`analysis-worker` (модуль `analysis-worker`):**
   - Перенос `analysis_kernel.rs`, `analysis_kernel_host.rs`, `static_analysis_packs.rs`, `code_diagnostics_feedback_loop.rs`, `architecture_snapshot.rs`, `architecture_snapshot_runtime.rs`.
   - Замена прямых вызовов из `evohime-core` на вызов `analysis-worker`.
3. **`verifier` (модуль `verifier`):**
   - Перенос `plan_review.rs`, `multi_reviewer_ensemble.rs`, `verification_evidence_ledger.rs`, `code_review_lane.rs`, `design_intent_review_lane.rs`.
   - Переключение `ipc_bridge_terminal_review.rs` и `ipc_bridge_ambient_plan_review.rs` на внешний сервис `verifier`.
4. **`hive` (модуль `hive`):**
   - Перенос командной логики из `hive_delegation.rs`, `hive_context.rs`, `team_coordinator.rs`, `team_coordination_policies.rs`, `team_resource_budget.rs`, `team_sop_protocols.rs`, не требующей прямого владения SQLite, в `hive` (с сохранением только IPC-проекции и permission gate в `evohime-core`).

## Проверка этапа
- Зелёные CI-пайплайны `evals.yml`, `analysis-worker.yml`, `verifier.yml`, `hive.yml`, `core.yml`.
- Отсутствие перенесённых `.rs` файлов и их тестов в `crates/evohime-core/src`.
