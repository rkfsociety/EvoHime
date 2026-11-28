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

## Статус проверки (2026-11-28, чтение кода + узкие локальные тесты)
- `verifier` — ✅: ревью и правка планов исполняются внешним сервисом. `evohime-verify.exe --serve` поднимает `evohime_service_contract`-сервис (`crates/evohime-receipts/src/verifier_service.rs`) с capability `verifier.plan_review_run` и `verifier.plan_revision_run`; `ipc_bridge_terminal_review.rs::start_plan_review`/`revise_plan` вызывают их через `ModuleRegistry::invoke_if_ready` (модуль добавлен в `SERVICE_MODULE_EXECUTABLES`). Ключ провайдера не покидает окружение: сервис строит `ModelGateway` из своего env и использует маршрут, указанный ядром; в payload передаётся только имя маршрута. Прогресс-события возвращаются одной пачкой в ответе (потокового Invoke в контракте сервисов пока нет), остановка ревью прекращает публикацию результатов, а идущий в сервисе прогон завершается сам — его результат отбрасывается. Консольный режим ядра (`test-agent.ps1 -ReviewPlan`) намеренно остаётся in-process: это dev-инструмент внутри самого ядра, а не мост UI.
- `evals` — ✅ с уточнением: детерминированный набор проверок (`evals.rs`, `evals_tests.rs`, `evals_workflow_orchestration.rs`) переехал в CLI-крейт `evohime-eval` (`crates/evohime-eval/src/evals*.rs`) и работает через `evohime_core`-контракты, поднятые до публичного API с `///`-документацией (`doctor`, `memory_domain`, `workflow_runner`, `workflow_registry`, `workflow_runtime`, `workflow_templates`, `test_sign_with_trusted_key`). Набор по-прежнему вызывается командой `evohime-eval` (`--mode deterministic`). IPC-сторона оценок (`evals.review_plan_*`, `evals.ensemble_*`, `evals.conformance_*`) обслуживается `evohime-evals-service` через `invoke_required_service`. В `crates/evohime-core/src` evals-файлов больше нет.
- `analysis-worker` — ✅ по существу: `static_analysis_packs`, `code_diagnostics_feedback_loop`, `architecture_snapshot*` перенесены; исполнение делегируется процессу `evohime-analysis-worker.exe` через supervisor-пайп (`analysis_kernel::supervisor_command`); оставшиеся в ядре `analysis_kernel.rs` + `analysis_kernel_host.rs` — тонкий фасад над `evohime-local-storage` и хост-клей журнала/tool-runtime, то есть материал микроядра.
- `hive` — ✅ по существу: `team_coordinator`, `team_coordination_policies`, `team_resource_budget`, `team_sop_protocols` перенесены в `evohime-hive-contract`; мёртвый `hive_context.rs` удалён из ядра; `hive_delegation.rs` (662 строки) остаётся — это гейтинг инструментов главного цикла агента (граница микроядра по 203.0), перенос его чистых хелперов потребовал бы зависимости `hive-contract` от `tool-runtime`.
- Дополнительная инфраструктурная правка: `StdioProcessTransport::send` ядра теперь выполняет блокирующий процессный ввод-вывод через `tokio::task::spawn_blocking` — длинные вызовы (ревью, grounded-исследования) больше не занимают воркер-поток tokio.
- Узкие локальные тесты: evals-service 26/26, receipts 81/81 (1 ignored) — зелёные. Полный CI-прогон подтверждает push.
