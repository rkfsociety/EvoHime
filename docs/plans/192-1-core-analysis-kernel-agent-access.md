# План 192.1 — декларация инструментов и host-контракт

Статус: active implementation contract. Этап следует после overview 192.0.

## Scope и изменяемые контракты

Объявить kernel в единственном месте — декларации builtin-инструментов в
`crates/tool-runtime`, — и определить, как Runtime получает доступ к
Core-owned kernel host, не заводя вторую реализацию dispatch.

1. Объявить инструмент `analysis.run` с явной input schema:
   `operation` (`json_parse` | `json_select` | `csv_summary` | `object_put`),
   `args` (bounded string для `csv_summary`/`json_parse` либо объект для
   `json_select`/`object_put`), опциональные `logical_name`, `type_hint`,
   `sensitivity`. `additionalProperties: false`, `required: ["operation"]`.
   Схема обязана проходить `ToolManifest::validate` и не содержать заглушки
   `{"input":{...}}`.
2. При необходимости отдельные узкие id (`analysis.json.select` и т.п.) не
   вводятся: один инструмент с `operation` проще для отбора и не раздувает
   каталог. Если при реализации окажется, что `csv_summary` и
   `json_select` конфликтуют по форме `args`, допускается разделить на два
   инструмента с отдельными схемами, но не более.
3. Ввести host-интерфейс исполнения kernel как **необязательную зависимость
   самого реестра**, а не поле `ToolContext`. Причина: `ToolContext`
   конструируется структурным литералом в ~70 местах в `crates` и
   `crates/tool-runtime/tests`, поэтому новое обязательное поле сломало бы их
   все и раздуло бы diff вне scope. Вместо этого:
   - `ToolRegistry` получает приватное поле
     `kernel_host: Option<Arc<dyn KernelHost>>` и builder
     `with_kernel_host(...)`, который возвращает `Self`; существующие
     конструкторы `new`/`with_permissions`/`bootstrap*` остаются совместимыми;
   - `Clone` для `ToolRegistry` продолжает работать, так как `Arc` клонируется.
   Trait `KernelHost` объявляется в `tool-runtime` с единственным методом вида
   `execute(&self, task: KernelTaskBinding, request: KernelToolRequest) ->
   impl Future<Output = Result<KernelToolResult, KernelToolError>>`.
   Тип непрозрачен для crate: известны только контракт и typed ошибки, ничего
   о SQLite или supervisor pipe.
4. Реализация `execute` в `tool-runtime` при `kernel_host = None` возвращает
   `ToolError::Execution("analysis_kernel_unavailable")` — без in-process
   fallback и без обхода worker.
5. Права: инструмент объявляет пустой список `Permission`-требований (kernel
   не даёт доступа к файлам, сети или shell), но исполнение выполняется в
   обычном `execute_with_cancellation`/`execute_after_durable_approval` пути,
   чтобы approval/receipt/recovery semantics остались неизменными. Отдельное
   permission не вводится: kernel-вызов не расширяет authority, а
   вычислительные операции side-effect-free.
6. `analysis.run` попадает в
   `crates/evohime-core/src/core_prelude.rs::catalog_preflight_input` с
   probe-аргументами, проходящими объявленную схему (`{"operation":
   "json_parse", "args": "{}"}`), иначе безопасный инструмент выпадет из
   authorized snapshot.
7. `Classification`: `capability_class = "local_computation"`,
   `side_effect = ReadOnly`, `approval = Never`, `workspace_scope =
   "task"` — как у существующих utility-инструментов с пустыми правами.

## Зависимости

### Блокирующие

`ToolManifest`/`builtin_input_schema`/`ToolManifest::validate`;
`ToolRegistry::register` и существующий dispatch в `registry.rs`; решение
плана 191.1 о единственной декларации (если 191.1 закрыт — расширяется его
каталог, если нет — текущий `registry.rs`).

### Опциональные

Разделение `analysis.run` на два инструмента при конфликте формы `args`.
Курируемый fallback из плана 191.2 — не блокирует.

## Recovery и rollback

Удаление объявления убирает инструмент из каталога и из модели; typed
`ToolError::UnknownTool` остаётся для старых вызовов. Отсутствие
`kernel_host` даёт typed `unavailable`. Никаких миграций данных и изменений
approval-таблиц нет. Rollback — снятие объявления и host-билдера целиком;
kernel IPC-команды продолжают работать.

## Verification

Unit: объявленная схема проходит `validate`, не является заглушкой, не
содержит `additionalProperties: true`; `catalog_preflight_input("analysis.run")`
проходит ту же схему; реестр без `kernel_host` возвращает `unavailable`;
`registry.list()` содержит `analysis.run` ровно один раз; `ToolRegistry::clone`
сохраняет host; существующие тесты `registry_tests.rs` не фиксируют литерал
числа инструментов.

Static: ссылки на `analysis.run` присутствуют в декларации, в
`catalog_preflight_input` и в проекции каталога; в
`core_agent_execution.rs` нет нового `if call.name == "analysis.run"`.

## Release evidence

Зафиксировать: id/версию объявленного инструмента, его canonical hash,
результат проверки схемы, mapping `operation → kernel operation`, отсутствие
новых permission-значений и изменений схемы SQLite. Плановые проверки не
выдавать за выполненные.

## Критерии выхода

- [ ] `analysis.run` объявлен в единственном месте и валиден.
- [ ] Схема совпадает с полями, которые реально разбирает адаптер.
- [ ] При отсутствии host — typed `unavailable`, без fallback.
- [ ] В `core_agent_execution.rs` нет нового дублирующего dispatch.
- [ ] `catalog_preflight_input` проходит схему инструмента.
- [ ] Внутренние ссылки разрешаются, `git diff --check` проходит.
