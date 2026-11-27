# План 203.0 — Фактический перенос вызовов из `evohime-core` в внешние сервисы и очистка микроядра

Статус: active implementation contract. Продолжение и фактическое завершение декомпозиции из [Плана 202](./202-0-core-microkernel-decomposition.md).

## Контекст и проблема
В рамках Плана 202 построены контракт `evohime-service-contract`, реестр `ModuleRegistry`, пакет данных `core-assets`, 12 изолированных сервисных модулей, единый манифест `modules.manifest.json`, CI для 26 модулей и точечный рестарт в `supervisor`/`updater`.
Однако в `crates/evohime-core/src` по-прежнему остаются **317 файлов**, включая крупные прикладные подсистемы (`workflow*`, `memory_extraction.rs`, `workspace_rag*`, `ambient.rs`, `image_generation.rs`, `research*`, `evals*`, `external_agent_a2a*`, `local_model_*`, `free_provider_reliability_routing.rs`), а прямые вызовы из `core_coordinator_*` и `ipc_bridge_*` всё ещё исполняются внутри ядра. Из-за этого `evohime-core` по-прежнему собирается и тестируется ~17 минут.

## Цель
Перенести фактическую бизнес-логику и unit/integration-тесты из `crates/evohime-core/src` в изолированные сервисы, переключить все вызовы ядра на `ModuleRegistry` и физически удалить перенесённые модули и лишние зависимости из `evohime-core`, превратив `evohime-core` в тонкое микроядро.

## Границы микроядра (не выносятся)
В `crates/evohime-core` остаются только:
- главный цикл агента, сессии и оркестрация (`core_agent*`, `core_coordinator*`, `core_lifecycle`, `core_prelude`);
- IPC и транспорт (`desktop-ipc`, `pipe_server`, `unix_pipe_server`, `core_protocol`, тонкие адаптеры `ipc_bridge*`);
- SQLite и журнал событий (`evohime-local-storage`, `core_journal*`, `evohime-receipts`);
- безопасность и политики (`permissions`, `policy_gate`, `sensitive_data_guardrails`, `output_guardrail_pipeline`);
- базовый маршрутизатор и реестр (`model-gateway`, `tool-runtime`, `context-budget`, `ModuleRegistry`, `core_assets`).

## Правила переноса каждой подсистемы
1. **Сначала перенос и тесты в сервисе:** вся логика и тесты переносятся в целевой крейт (без зависимости от `evohime-core`), контрактные capability регистрируются через `evohime-service-contract`.
2. **Переключение вызовов в `core`:** прямые вызовы в `core_coordinator_*`, `core_agent_*` и `ipc_bridge_*` заменяются на `ModuleRegistry::invoke_if_ready(...)` с typed `Unavailable` fallback при недоступности сервиса.
3. **Физическое удаление из `evohime-core`:** старые `.rs` файлы, `pub mod` в `lib.rs` и тесты удаляются из `crates/evohime-core`; при необходимости контракты хранения в `evohime-local-storage` остаются без дублирования бизнес-логики.
4. **Изоляция CI:** изменения внутри конкретного сервиса обновляют только `release-versions/<module>.txt` и `release-notes/<module>.md` этого сервиса и не трогают `evohime-core` после завершения волны.

## Этапы (волны переноса)
- [Этап 1 — Неиспользуемые контракты и слабосвязанные сервисы (`computer-use`, `ambient`, `media-voice`, `research`)](./203-1-core-service-migration-and-cleanup.md)
- [Этап 2 — `evals` и доперенос в `analysis-worker`, `verifier`, `hive`](./203-2-core-service-migration-and-cleanup.md)
- [Этап 3 — Тяжёлые подсистемы (`rag-memory`, `remote-a2a`, `workflow-engine`)](./203-3-core-service-migration-and-cleanup.md)
- [Этап 4 — `model-router`, `tools-workspace`/`tools-exec`, зачистка `Cargo.toml` ядра и проверка скорости CI](./203-4-core-service-migration-and-cleanup.md)

## Блокирующие зависимости
- [План 202](./202-0-core-microkernel-decomposition.md) — базовая инфраструктура микроядра и `ModuleRegistry` (готова).

## Критерии готовности плана
- [ ] Все перечисленные в этапах 1–4 прикладные файлы и их тесты удалены из `crates/evohime-core/src` (прогресс: перенесены и удалены из `evohime-core` 34 модуля — к 32 модулям добавлены `batch_invocation_runtime` и `policy_aware_tool_result_cache`).
- [ ] Все соответствующие вызовы в `core_coordinator_*`, `core_agent_*` и `ipc_bridge_*` работают через `ModuleRegistry` (прогресс: `ModuleRegistry` подключён в `IpcBridge`, переключены `voice_input_dictation`, `project_knowledge_notebook`, `offline_experience_consolidation`, `experience_replay_library`, `research.fetch_url`, `research.grounded_session`, `workflow.task_graph_*`, `workflow.batch_*`, `evals.review_plan_*`, `evals.ensemble_*`, `tools.diagnostics_*`, `tools.analysis_pack_*`, `tools_exec.cache_*`, `model_router.operator_*`, `model_router.guardrail_*`).
- [ ] Неиспользуемые зависимости удалены из `crates/evohime-core/Cargo.toml`.
- [ ] Полный прогон CI всех 26 модулей и `Rust documentation` зелёный; время сборки/тестов `core.yml` заметно сокращено и зафиксировано в `docs/release-evidence.md`.
