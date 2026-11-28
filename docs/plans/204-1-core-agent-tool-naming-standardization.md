# План 204.1 — `tool-runtime`: константы, JSON-схемы, `glob` и консолидация `bash`

## Scope
Переименование инструментов и обновление их контрактов на уровне `crates/tool-runtime`:
- константы `NAME` в `crates/tool-runtime/src/tools/*.rs`;
- JSON-схемы в `crates/tool-runtime/schemas/` и генерация в `manifest.rs`;
- `registry.rs`, `risk.rs`, `sandbox.rs`, `shell_env.rs`, `execution_policy_profiles.rs`;
- расширение `glob` (бывший `filesystem_list`) поддержкой glob-масок путей (`**/*.rs`, `crates/*/Cargo.toml`) с сохранением безопасного обхода в пределах `WorkspaceSandbox`;
- консолидация запуска команд: `bash` (бывший `shell_execute`) как единый основной инструмент исполнения команд; устранение дублирования с `process_run` (или чёткое разграничение без пересечения для модели).

## Зависимости
- **Blocking:** [План 204.0](./204-0-core-agent-tool-naming-standardization.md).
- **Optional:** нет.

## Изменяемые контракты
- `crates/tool-runtime/src/tools/{filesystem,write,patch,search,list,shell,http,filesystem_advanced,process,script}.rs`
- `crates/tool-runtime/schemas/*.json`
- `crates/tool-runtime/src/manifest.rs`, `registry.rs`, `risk.rs`, `execution_policy_profiles.rs`

## Recovery и безопасность
- Сохраняются `WorkspaceSandbox`, allowlist/denylist команд, `ExecutionPolicyProfile` (таймауты до `300_000` мс), проверка `expected_hash` для `write`/`edit`.
- `resolve_tool_id` не возвращает и не пробрасывает старые имена `filesystem_*` / `shell_execute`.

## Verification
- Обновление `crates/tool-runtime/src/registry_tests.rs`, `crates/tool-runtime/tests/stage3_tools.rs` и unit-тестов каждого модуля.
- Проверка `cargo test -p evohime-tool-runtime`.

## Rollback
- Откат коммита этапа в `crates/tool-runtime` до интеграции с `evohime-core`.
