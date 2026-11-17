# План 196.1 — канонический лексикон и удаление трансляции

Статус: active implementation contract. Этап следует после overview 196.0.

## Scope и изменяемые контракты
1. Переименовать `*_NAME` константы в `crates/tool-runtime/src/tools/*.rs`
   (24 файла, 64 id) и `developer_utilities.rs` (9 `utility.*`): разделитель
   `.` заменяется на `_`.
2. Обновить потребителей id в `tool-runtime`: `registry.rs` (регистрация,
   `manifest()`, `manifest_for`), `manifest.rs` (ветки `builtin_input_schema`),
   `risk.rs` (`classify_call_risk`), `execution_policy_profiles.rs`.
3. Обновить id-константы и тексты в `evohime-core`: `core_prelude.rs`
   (`TOOL_*`, `DELIVERY_*`, `FULL_ACCESS_WORKING_SET`, `WORKFLOW_TOOL_IDS`,
   `catalog_preflight_input`, примеры в `build_agent_system_prompt`),
   `recovery.rs` (`READ_ONLY_SIGNATURE_PREFIXES`), `provider_resilience.rs`,
   `legacy_parser.rs` (`LEGACY_TOOL_NAMES`), `core_agent_execution.rs`.
4. Удалить `provider_function_name` из `crates/model-gateway/src/tools.rs` и его
   вызовы в `providers/literouter.rs` и `providers/openai_responses.rs`.
   Заменить тест замены точки тестом тождества: имя в wire-запросе равно
   `manifest.tool_id`.
5. Удалить `canonical_tool_call_name` и вызов в
   `core_agent_execution.rs`; на этом этапе точные имена совпадают, поэтому
   резолв не нужен. Появление legacy-входа обрабатывает этап 2.
6. Исправить эвристики, выводившие смысл из точки, — не механической заменой:
   - `context_budget.rs`: `capability = name.split('.').next()` и маркеры
     `MANDATORY_CAPABILITY_MARKERS`/`MUTATION_MARKERS`;
   - `registry.rs`: `starts_with("utility.")` / `starts_with("workflow.")`;
   - `trace_diagnostics.rs`: единственная точка, где уже принимались обе формы;
   - `conversation_event_log.rs`: категории по префиксам;
   - `legacy_parser.rs`: `!name.starts_with("git.")`;
   - `ipc_bridge_ambient_transport.rs`: сравнение `capability_ref` с `shell`;
   - `core_agent.rs`: синтетическое `"shell.execute: {cmd}"`;
   - renderer `chat-summary.ts`, `tool-names.ts`.
7. Обновить in-crate тесты, где id — литерал: `core_tests.rs`,
   `registry_tests.rs`, `tools/*` тесты, `model-gateway` provider-тесты.

## Зависимости
### Блокирующие
Overview 196.0; `ToolManifest`/`ToolRegistry`; manifest schemas; CI
`tool-runtime` и `evohime-core` contract tests.
### Опциональные
Обновление документации (этап 4) и UI-словаря (этап 3); до них id в текстах
может оставаться точечным, но код обязан быть согласован.

## Recovery и rollback
Этап меняет только имена, не состав и не authority. Любой неизвестный id по-
прежнему даёт `UnknownTool`. Если CI обнаруживает пропущенный литерал,
исправление входит в тот же этап: смешанный словарь в одном модуле запрещён.
Rollback — возврат предыдущей версии модуля; durable-состояние этап не пишет.

## Verification
- `cargo test -p evohime-tool-runtime` и `cargo test -p evohime-core` зелёные
  на новом лексиконе.
- Тест реестра: каждый зарегистрированный id соответствует
  `^[a-z][a-z0-9_]*$`, точек нет.
- Тест provider wire-format: сериализованный `tools[].function.name` равен
  `manifest.tool_id` без преобразования.
- Тест эвристик: capability и категория выводятся из канонического id так же,
  как раньше из точечного.

## Release evidence
Список переименованных id (73 константы), diff `EXPECTED_TOOL_COUNT`
(не меняется), вывод CI по `tool-runtime`, `evohime-core` и `model-gateway`.

## Критерии выхода
- [ ] В коде нет точечных tool id, кроме legacy-таблиц этапа 2.
- [ ] `provider_function_name` и `canonical_tool_call_name` удалены.
- [ ] Ни одна эвристика не зависит от позиции точки.
- [ ] Wire-имя равно каноническому id.
