# План 203.4 — Волна 4: `model-router`, инструментальные сервисы, очистка `Cargo.toml` ядра и закрытие

## Цель этапа
Завершить перенос оставшихся маршрутизирующих и инструментальных модулей, очистить `crates/evohime-core/Cargo.toml` от лишних зависимостей и зафиксировать реальное сокращение времени сборки и тестирования `evohime-core`.

## Состав работ
1. **`model-router` → `crates/evohime-model-router` (`evohime-model-router.exe`):**
   - Перенос `local_model_adaptation.rs` (81.7 KB), `local_model_runtime_manager.rs`, `local_model_compatibility_gateway.rs`, `local_model_performance_calibration.rs`, `local_inference_scheduler.rs`, `free_access_probe.rs`, `free_provider_reliability_routing.rs` (93.9 KB), `free_provider_reliability_routing_tests.rs`.
   - Переключение `core_coordinator_routing.rs` и `ipc_bridge_local_model_adaptation_scheduler.rs` на `ModuleRegistry` (с учётом бюджета IPC-латентности на шаг диалога; быстрая таблица маршрутизации остаётся в `model-gateway`).
2. **`model-adapters`, `tools-workspace`, `tools-exec`:**
   - Финальная передача `legacy_parser.rs` (и смежных адаптеров парсинга вызовов) в `evohime-model-adapters`, файловых/git операций в `evohime-tools-workspace`, shell/process операций в `evohime-tools-exec` с проксированием из `tool-runtime` / `evohime-core` через `ModuleRegistry` с проверкой `permissions` в ядре.
3. **Зачистка `crates/evohime-core/Cargo.toml` и `.github/workflows/core.yml`:**
   - Удаление неиспользуемых зависимостей (Win32 GUI, парсеры/HTTP/инструментальные крейты, переехавшие в сервисы) из `crates/evohime-core/Cargo.toml`.
   - Проверка, что `core.yml` тестирует только микроядро и его базовые контракты без пересборки прикладных сервисов.
4. **Финальная верификация и закрытие Планов 202 и 203:**
   - Сравнительный замер времени `core.yml` (до и после очистки) и времени сборки изолированных сервисов в GitHub Actions.
   - Проверка `cargo doc` gates, `rustfmt`, `clippy`, `documentation.tests.ps1`, `check-modules-manifest.ps1`, `compatible-manifest.yml`.
   - Обновление `docs/architecture.md`, `docs/current-state.md`, `docs/release-evidence.md` и удаление временных файлов `203-*` после полного подтверждения в CI.

## Критерии готовности
- [ ] `crates/evohime-core/src` содержит только микроядро (IPC, SQLite/journal, permissions/guardrails, главный цикл агента, `ModuleRegistry`).
- [ ] Прямые вызовы вынесенных доменов в `evohime-core` полностью переведены на `ModuleRegistry`.
- [ ] Лишние зависимости удалены из `crates/evohime-core/Cargo.toml`.
- [ ] Все 26 модулей и `Rust documentation` в CI зелёные, ускорение `core.yml` зафиксировано в `docs/release-evidence.md`.
