# План 196.2 — legacy-резолвер на входных границах

Статус: active implementation contract. Этап следует после 196.1.

## Scope и изменяемые контракты
1. Добавить в `tool-runtime` единственный резолвер
   `resolve_tool_id(name: &str) -> Option<&'static str>`:
   - точное совпадение с зарегистрированным id;
   - иначе legacy-форма с точкой, если она однозначно соответствует одному id;
   - иначе `None`. Неоднозначность (`a_b.c` против `a.b_c`) не разрешается
     угадыванием.
2. Применять резолвер на каждой входной границе, где имя приходит извне:
   - `ToolRegistry::manifest_for`, `preflight`, `execute_with_cancellation`;
   - текстовые tool-call (`legacy_parser.rs`);
   - имя, возвращённое провайдером (`core_agent_execution.rs`);
   - `ReconcilePendingReceiptAction.tool_name` (IPC);
   - `SkillMetadata.allowed_tools` и `SkillRegistry` frontmatter;
   - `GateV1.capability_ref` континуации (`ipc_bridge_ambient_transport.rs`);
   - workspace-set grants и `CapabilityManifest.allowed_tools`;
   - `prompt_strategy` профили `required_tool_ids`.
3. Запись legacy-формы в новое состояние запрещена: канонизация выполняется
   до journal/receipt insert, поэтому `tool_intents.tool_name` и
   `receipt_actions.tool_name` получают только канонический id.
4. Ошибка неизвестного имени остаётся `UnknownTool` и называет каноническую
   форму, если legacy-алиас существует.

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
- Unit: точное имя, одна legacy-форма, неоднозначная пара, неизвестное имя.
- Test: резолв ограничен активным loadout — legacy-имя выключенного инструмента
  не становится доступным.
- Test: `link_tool_receipt` находит цель и для legacy-строки в истории.
- Test: continuation policy с точечным `capability_ref` резолвится, а не даёт
  `gate_unavailable`.
- Test: skill/capability пакет с `allowed-tools: filesystem.patch` загружается.

## Release evidence
Таблица legacy→canonical, результаты тестов резолвера, replay исторической
receipt-цепочки и вывод CI по `evohime-core`, `evohime-receipts`,
`evohime-local-storage`.

## Критерии выхода
- [ ] Резолвер один и живёт в `tool-runtime`.
- [ ] Legacy-имя принимается на каждой входной границе и не пишется в новое
      состояние.
- [ ] Неоднозначная legacy-форма отклоняется.
- [ ] Historical receipts и continuation policies остаются рабочими.
