# План 196.2 — legacy-резолвер на входных границах

Статус: active implementation contract. Этап следует после 196.1.

## Scope и изменяемые контракты
1. Резолвер живёт в `tool-runtime` и состоит из трёх функций:
   - `ToolRegistry::resolve_tool_id(name) -> Option<&'static str>` — точное
     совпадение с зарегистрированным id, иначе лексическая замена `.` на `_` с
     повторной проверкой по реестру, иначе `None`. Алиас не может включить
     незарегистрированный инструмент.
   - `canonical_tool_id(name) -> String` — лексическая нормализация списков в
     сохранённых пакетах (capability, skill, prompt-strategy).
   - `tool_namespace(name) -> &str` — namespace для группировки и фильтров;
     принимает обе формы, поэтому историческое событие не теряет категорию.
2. Резолвер применяется на каждой входной границе, где имя приходит извне:
   - `ToolRegistry::manifest_for`, `preflight`, `execute_with_cancellation`,
     `execute_after_durable_approval`, `execute_after_approval`;
   - текстовые tool-call (`legacy_parser.rs`): `canonical_tool_name`
     нормализует имя до создания `NativeToolCall`;
   - имя, возвращённое провайдером (`core_agent_execution.rs`) —
     `adaptive_tool_catalog::resolve_tool_call_name`, только активный loadout;
   - `ReconcilePendingReceiptAction.tool_name` (IPC) — через реестр;
   - `SkillMetadata.allowed_tools` и frontmatter `allowed-tools`;
   - `GateV1.capability_ref` континуации (`ipc_bridge_ambient_transport.rs`) —
     через `manifest_for` и namespace-проверку `shell`;
   - workspace-set grants (значение допустимо, исполнение идёт через реестр);
   - `CapabilityManifest.allowed_tools`/`required_tools` и
     `prompt_strategy` `required_tool_ids` — сравнение по
     `canonical_tool_id`;
   - capability-имена дочерних задач (`child_executor::canonical_capability`
     сохраняет точечные алиасы для durable запросов).
3. Запись legacy-формы в новое состояние запрещена: канонизация выполняется
   до journal/receipt insert, поэтому `tool_intents.tool_name` и
   `receipt_actions.tool_name` получают только канонический id.
4. Неизвестное имя остаётся `UnknownTool`. Резолвер не подсказывает и не
   угадывает каноническую форму: он либо знает id, либо отказывает.

## Зависимости
### Блокирующие
196.1 (канонический лексикон); `evohime-receipts` schema; IPC proto;
`local-storage` `model_provenance`/`continuation_store`.
### Опциональные
196.3 UI-словарь может показывать legacy-подпись, но не влияет на резолв.

## Recovery и rollback
Резолвер не создаёт capability: он отображает строку на существующий id и
возвращает `None` для всего остального, поэтому fail-closed сохраняется.
Исторические записи не переписываются; чтение принимает обе формы.
Rollback удаляет резолвер и оставляет 196.1 — код перестаёт понимать старые
имена, что видно в тестах и не портит данные.

## Verification
- Unit: точное имя, legacy-форма, имя вне реестра, точечное имя без
  соответствующего id (`git.unknown`).
- Unit: `canonical_tool_id` меняет только разделитель.
- Unit: `resolve_tool_call_name` не возвращает id вне активного loadout.
- Test: `manifest_for("git.diff")` отдаёт манифест `git_diff`.
- Test: continuation policy с точечным `capability_ref` резолвится, а не даёт
  `gate_unavailable`.
- Test: skill/capability пакет с `allowed-tools: filesystem.patch` загружается
  и пересекается с каноническим loadout.

## Release evidence
Таблица legacy→canonical, результаты тестов резолвера, replay исторической
receipt-цепочки и вывод CI по `evohime-core`, `evohime-receipts`,
`evohime-local-storage`.

## Критерии выхода
- [x] Резолвер один и живёт в `tool-runtime`.
- [x] Legacy-имя принимается на каждой входной границе и не пишется в новое
      состояние.
- [x] Неоднозначности нет: замена разделителя инъективна, а имя вне реестра
      отклоняется.
- [ ] Historical receipts и continuation policies подтверждены тестами CI.
