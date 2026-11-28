# План 204.2 — `evohime-core` и смежные Rust-крейты: каталог, права, квитанции и recovery

## Scope
Переключение ядра и вспомогательных Rust-крейтов на новые канонические имена инструментов:
- `crates/evohime-core`: `adaptive_tool_catalog.rs`, `core_agent_execution.rs`, `core_agent.rs`, `core_prelude.rs`, `recovery.rs`, `hive_delegation.rs`, `policy_gate.rs`, `permission_rules.rs`, `trace_diagnostics.rs`, `evals.rs`, `workspace_bootstrap_manifest.rs`, `ipc_bridge_*.rs`;
- `crates/permissions`: правила и тесты разрешений;
- `crates/evohime-receipts`: подписи и capability-контракты (`capability.rs`, `runtime_signing.rs`, `runtime_tests.rs`);
- `crates/evohime-local-storage`: тестовые контракты (`tests_runtime.rs`, `tests_schema.rs`);
- `crates/context-budget`, `crates/model-gateway`, `crates/evohime-tools-exec`: loadout, tool schemas и policy-aware кэш.

## Зависимости
- **Blocking:** [План 204.1](./204-1-core-agent-tool-naming-standardization.md).
- **Optional:** нет.

## Изменяемые контракты
- Внутренний каталог инструментов (`adaptive_tool_catalog`), правила отслеживания ревизий файлов (`read` / `write` / `edit`), правила `git_commit` gate (обязательная проверка через `bash` / `cargo_test` / `script_execute`), маппинг ошибок в `recovery.rs`.
- Системные инструкции модели в `core_prelude.rs` и `hive_delegation.rs`: упоминаются только новые имена (`read`, `write`, `edit`, `grep`, `glob`, `bash`, `web_fetch`).

## Recovery и безопасность
- Все политики разрешений (`Permission::FilesystemRead`, `FilesystemWrite`, `ShellExecute`, и т.д.) остаются привязаны к соответствующим инструментам без ослабления.
- `recovery.rs` формирует подсказки с новыми именами и параметрами.

## Verification
- Прогон unit-тестов в `evohime-core`, `permissions`, `evohime-receipts`, `evohime-local-storage`, `context-budget`, `model-gateway`, `evohime-tools-exec` в CI.

## Release evidence
- Зелёные проверки `core.yml`, `tools-exec.yml` и `Rust documentation` в GitHub Actions.

## Rollback
- Откат коммита этапа (совместно с этапом 1 при необходимости).
