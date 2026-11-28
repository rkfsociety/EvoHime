# План 204.1 — `tool-runtime`: константы, JSON-схемы, `glob` и консолидация `bash`

## Scope
Переименование инструментов и обновление их контрактов на уровне `crates/tool-runtime`:
- константы `NAME` в `crates/tool-runtime/src/tools/*.rs`;
- переименование JSON-схем в `crates/tool-runtime/schemas/` (`filesystem_read.json` → `read.json`, `filesystem_write.json` → `write.json`, `filesystem_patch.json` → `edit.json`, `filesystem_search.json` → `grep.json`, `shell_execute.json` → `bash.json`) и добавление `glob.json`, плюс генерация в `manifest.rs`;
- `registry.rs`, `risk.rs`, `sandbox.rs`, `shell_env.rs`, `execution_policy_profiles.rs`;
- расширение `glob` (бывший `filesystem_list`) поддержкой glob-масок путей (`**/*.rs`, `crates/*/Cargo.toml`) с сохранением безопасного обхода в пределах `WorkspaceSandbox`;
- консолидация запуска команд: `bash` (бывший `shell_execute`) как единый основной инструмент исполнения команд; выведение дубля `process_run` из реестра и политик при сохранении `script_execute` для `.ps1`/`.py`/`.js` скриптов.

## Зависимости
- **Blocking:** [План 204.0](./204-0-core-agent-tool-naming-standardization.md).
- **Optional:** нет.

## Изменяемые контракты
- `crates/tool-runtime/src/tools/{filesystem,write,patch,search,list,shell,http,filesystem_advanced,process,script}.rs`
- `crates/tool-runtime/schemas/{read,write,edit,grep,glob,bash}.json`
- `crates/tool-runtime/src/manifest.rs`, `registry.rs`, `risk.rs`, `execution_policy_profiles.rs`

## Recovery и безопасность
- Сохраняются `WorkspaceSandbox`, allowlist/denylist команд, `ExecutionPolicyProfile` (таймауты до `300_000` мс), проверка `expected_hash` для `write`/`edit`.
- `resolve_tool_id` не возвращает и не пробрасывает старые имена `filesystem_*` / `shell_execute` / `process_run`.

## Verification
- Обновление `crates/tool-runtime/src/registry_tests.rs`, `crates/tool-runtime/tests/stage3_tools.rs` и unit-тестов каждого модуля.
- Проверка `cargo test -p evohime-tool-runtime` в CI.

## Release evidence
- Зелёный `cargo test -p evohime-tool-runtime` и `Rust documentation` в рамках `core.yml` / `windows.yml`.

## Rollback
- Откат коммита этапа в `crates/tool-runtime` до интеграции с `evohime-core`.
