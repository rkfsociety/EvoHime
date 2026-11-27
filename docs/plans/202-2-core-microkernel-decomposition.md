# План 202.2 — Доперенос в существующие модули и первая волна (`evals`, `media-voice`, `research`)

## Цель этапа
Сократить объём `evohime-core` на первом шаге за счёт самых слабосвязанных подсистем и доукомплектования уже существующих модулей.

## Области переноса
1. **В существующие модули:**
   - `analysis-worker`: `analysis_kernel.rs`, `analysis_kernel_host.rs`, `static_analysis_packs.rs`, `code_diagnostics_feedback_loop.rs`, `architecture_snapshot*.rs`.
   - `hive`: `hive_delegation.rs`, `hive_context.rs` и командная координация (`team_coordinator*`, `team_sop_protocols.rs`, `team_resource_budget.rs`), не требующая прямого владения SQLite.
   - `verifier`: `plan_review.rs`, `multi_reviewer_ensemble.rs`, `deterministic_review_execution_plan.rs`, `verification_evidence_ledger.rs`, `code_review_lane.rs`, `design_intent_review_lane.rs`.
2. **Новые модули-сервисы (на базе `evohime-service-contract`):**
   - `evals` (`crates/evohime-eval` → бинарник `evohime-evals.exe`): бенчмарки, симуляция инструментов, калибровка локальных моделей и железа.
   - `media-voice` (`crates/evohime-media-voice` → `evohime-media-voice.exe`): генерация изображений, `vision_contract`, голосовые команды и озвучка.
   - `research` (`crates/evohime-research` → `evohime-research.exe`): веб-поиск, fetch, research pipeline, браузерные сессии, публичный GitHub.

## Изменяемые контракты
- Новые записи в едином реестре модулей, `release-versions/<module>.txt`, `release-notes/<module>.md`, `.github/workflows/<module>.yml` (через `module-native.yml`).
- Замена прямых вызовов этих модулей внутри `evohime-core` на вызовы через `ModuleProxy`.

## Зависимости
- Блокирующая: Этап 202.1 (`evohime-service-contract`, `ModuleRegistry`).

## Recovery и rollback
- Если модуль не запущен или упал во время задачи, `ModuleProxy` возвращает typed `unavailable` без падения `evohime-core`, а `supervisor` перезапускает модуль по backoff-политике.

## Проверка
- Изолированные `cargo test` для каждого нового/обновлённого крейта.
- Контрактные тесты взаимодействия `evohime-core` ↔ `evals` / `media-voice` / `research` через `evohime-service-contract`.
