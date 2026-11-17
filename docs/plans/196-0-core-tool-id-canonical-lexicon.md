# План 196.0 — Канонический лексикон tool id без подмены имён

Статус: active implementation contract. План использует существующий ToolRegistry,
ToolManifest, manifest schemas и permission owner; второго реестра не создаёт.

## Цель
Сделать одно имя инструмента на всём пути: id `filesystem_read` объявляется
модели, приходит от провайдера, попадает в policy, receipt, журнал и
документацию без трансляции. Точки остаются только как legacy-алиас на чтение.

## Основание checkout
До этого изменения действовали два представления одного id.
`provider_function_name` в `crates/model-gateway/src/tools.rs` заменял точки на
подчёркивания перед запросом к провайдеру, а
`adaptive_tool_catalog::canonical_tool_call_name` возвращал id обратно по
активному loadout. Проверка подтвердила: OpenAI и DeepSeek отклоняют точечные
имена с HTTP 400, поэтому трансляция была не косметикой, а обходом ограничения
провайдера. Пара функций создавала класс ошибок «модель вернула
`filesystem_read`, Core ищет `filesystem.read`» и делала логи, prompt и
provider-запрос разными словарями.

В исходниках уже: все 73 зарегистрированных id переименованы в `_`-форму,
`provider_function_name` удалён, `canonical_tool_call_name` заменён узким
`resolve_tool_call_name`, добавлены `ToolRegistry::resolve_tool_id`,
`canonical_tool_id` и `tool_namespace`, эвристики на точке переведены на
namespace. Осталось подтверждение CI и закрытие плана.

## Граница
- Канонический id — lowercase `[a-z0-9_]+`, разделитель `_`; точки запрещены.
- `filesystem.patch` → `filesystem_patch` и аналогично для всех 64
  зарегистрированных инструментов и 9 `utility.*`.
- Приём legacy-форм — только на входных границах и только на чтение.
- Новые записи в SQLite, receipts, журналы и provider-запросы содержат
  исключительно канонический id.
- Permission model не меняется: `permissions.json` адресует `Permission`-enum,
  а не tool id, поэтому пользовательские правила не мигрируют.
- Имена IPC-событий (`git.status`, `memory.search`, `workflow.started`) и
  IPC-команд Electron (`workflow.start`, `workflow.cancel`) — отдельный
  лексикон, они сохраняют прежнюю форму.
- `EXPECTED_TOOL_COUNT` остаётся прежним: меняются имена, не состав.

## Изменяемые контракты
- `ToolDefinition.name`, `ToolManifest.tool_id` и все `*_NAME` константы.
- `builtin_input_schema`, `risk::classify_call_risk`,
  `execution_policy_profiles`, `FULL_ACCESS_WORKING_SET`,
  `DELIVERY_*`, `LEGACY_TOOL_NAMES`, `read-only` списки receipts.
- Резолверы: `ToolRegistry::resolve_tool_id` (имя → зарегистрированный id),
  `canonical_tool_id` (лексическая нормализация сохранённых списков),
  `tool_namespace` (namespace для группировки и фильтров),
  `adaptive_tool_catalog::resolve_tool_call_name` (только активный loadout).
- Эвристики, выводившие смысл из точки: `context_budget` (`split('.')`,
  маркеры), `registry` (`starts_with("utility.")`/`"workflow."`/`"browser."`),
  `trace_diagnostics`, `conversation_event_log`, `legacy_parser`,
  `core_agent_execution` (`starts_with("workflow.")`),
  `ipc_bridge_ambient_transport` (continuation gate),
  renderer `chat-summary.ts`, `tool-names.ts`.

## Зависимости
### Блокирующие
ToolRegistry/ToolManifest и manifest schemas; model-gateway providers;
permission/approval/preflight; local-storage и receipts schema; desktop IPC
proto и генерируемые типы; существующий план 191 (tool surface).

### Опциональные
Планы 192–195 называют id в декларациях и планах; их тексты обновляются в
этапе 4, поведение не зависит. Renderer-словарь `tool-names.ts` полезен, но не
блокирует Core.

## Recovery и rollback
Неизвестный id отклоняется с `UnknownTool`. Legacy-алиас не расширяет
authority: `resolve_tool_id` возвращает только зарегистрированный id, а
`resolve_tool_call_name` работает только внутри активного loadout.
Исторические receipts остаются валидными, потому что подпись покрывает
сохранённые строки; проверки по имени принимают обе формы.
Rollback — возврат предыдущей версии модуля; durable-строки не переписываются.

## Этапы
- [Этап 1 — канонический лексикон и удаление трансляции](./196-1-core-tool-id-canonical-lexicon.md)
- [Этап 2 — legacy-резолвер на входных границах](./196-2-core-tool-id-canonical-lexicon.md)
- [Этап 3 — durable-совместимость, IPC и UI-проекции](./196-3-core-tool-id-canonical-lexicon.md)
- [Этап 4 — документация, evals, тесты и release evidence](./196-4-core-tool-id-canonical-lexicon.md)

## Критерии готовности
- [x] В provider-запрос уходит ровно канонический id, трансляции нет.
- [x] `provider_function_name` и `canonical_tool_call_name` удалены, их тесты
      заменены проверкой тождества id и приёма legacy-формы.
- [x] `permissions.json` и `workspaces.json`/`chats.json` не требуют миграции.
- [x] Legacy `filesystem.read` принимается на каждой входной границе и никогда
      не пишется в новое состояние.
- [x] Ни одна эвристика не выводит capability из позиции точки.
- [ ] Документация, планы 191–195 и eval-фикстуры согласованы с новым id.

## Non-goals
Смена permission enum, состава инструментов или `EXPECTED_TOOL_COUNT`;
перезапись исторических receipts и журналов; новые алиасы сверх legacy-точек;
второй реестр, router или словарь подмены имён.

## Verification и release evidence
Прогон registry/manifest contract-тестов, provider wire-format тестов,
resolver unit-тестов (точная форма, legacy-форма, имя вне loadout), receipt
replay на исторической цепочке и Electron-тестов словаря. Хранить: таблицу
соответствия legacy→canonical, результаты `check:protocol`, вывод CI по
затронутым модулям и `git ls-remote` подтверждение публикации.

## Критерии выхода overview
- [x] Одно имя инструмента во всех слоях.
- [x] Legacy-совместимость доказана тестами, а не комментарием.
- [x] Отличие id от capability class объяснено в архитектуре.
