# План 05.4 — Evidence provenance

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — явный provenance для model-visible контекста: типизированные ссылки на источники, DAG происхождения производных элементов и правила захвата workspace, memory и child-данных.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — поле `context_projection` и лимиты `MAX_EVIDENCE_REFS`, `MAX_SOURCE_REFS_PER_ENTRY`;
- [05.2](05-2-durable-storage.md) — таблица `model_request_sources`;
- [05.3](05-3-request-integration.md) — чокпойнт, в котором evidence разрешается перед commit;
- существующие Memory Extraction, Local Agentic RAG и child workflows как источники.

### Опциональные

- [05.6](05-6-compaction-shadowing.md) — append-only shadowing. До неё допустим переходный режим для уже созданных записей: при реконструкции старого запроса вытесненный original может вернуть `REQUEST_SOURCE_MISSING`. Это не разрешает создавать новый committed request с висячей ссылкой и не является допустимым состоянием после включения 05.6: shadowing обязан сохранять originals. После redaction/retention используются отдельные typed-состояния `REQUEST_REDACTED` и `REQUEST_RETENTION_PRUNED`.

## ContextEvidenceRef

```text
ContextEvidenceRef {
    kind
    source_id
    source_version?
    source_hash
    classification
    projection
}
```

`ContextEvidenceRef` не является вторым списком контекста. Это тип элемента
`ContextProjection.entries[].source_refs[]` из [05.1](05-1-canonical-request-contract.md)
и [05.6](05-6-compaction-shadowing.md). `projection` — ссылка на ту же
projection через `ledger_id`/`context_projection_hash` и
`projection_entry_id`; отдельный `projection_id` не вводится, а содержимое
projection в ссылке не дублируется.

Маппинг на нормализованную таблицу [05.2](05-2-durable-storage.md) такой:

```text
ContextEvidenceRef.kind          -> model_request_sources.source_kind
ContextEvidenceRef.source_id     -> model_request_sources.source_id
ContextEvidenceRef.source_version -> model_request_sources.source_version
ContextEvidenceRef.source_hash   -> model_request_sources.source_hash
```

`classification` и `projection` являются свойствами конкретного
model-visible envelope и сохраняются в каноническом `envelope_blob` внутри
`context_projection.entries[].source_refs[]`. Отдельная копия описания
контекста в `model_request_sources` не создаётся. Если storage-запросу нужны
эти свойства, он читает и проверяет envelope, а не заводит независимый
контекстный реестр.

`classification` — Core-owned уровень чувствительности источника:
`public`, `sensitive` или `secret`. Он не позволяет renderer понизить уровень
и не отменяет пользовательское удаление. Redaction, export и retention
применяют к нему соответствующую policy; `secret` не означает, что источник
можно оставить в provenance после удаления.

`source_version` обязателен для изменяемых или версионируемых источников:
для memory это `revision`, для child — `child_revision`, для workspace —
идентификатор captured snapshot/chunk, для plan review — immutable revision
плана. Для intrinsically immutable источника (`core_static`) версия может быть
отсутствующей только если его `source_hash` и immutable artifact ref однозначно
идентифицируют содержимое. В логическом `ContextEvidenceRef` `source_hash`
обязателен для intact hashable evidence при commit. Физическая колонка
`model_request_sources.source_hash` nullable только после разрешённого
удаления ambient-источника или `forget_window`; для `forget` памяти в ней
сохраняется digest согласно [05.8](05-8-redaction-and-retention.md).

Минимальные `kind`:

```text
conversation_event
memory
workspace_file
workspace_index
child_report
plan_review
system_context
compaction
tool_result
generated_summary
core_static
```

Смысл значений: `conversation_event` — зафиксированное событие диалога;
`memory` — конкретная revision памяти; `workspace_file` — captured bytes
файла; `workspace_index` — immutable результат индексации/поиска;
`child_report` — принятая revision отчёта child-задачи; `plan_review` —
зафиксированный reviewed plan и его revision; `system_context` — Core-owned
системный контекст; `compaction` — результат операции сжатия; `tool_result` —
зафиксированный результат инструмента; `generated_summary` — производное
резюме; `core_static` — неизменяемый Core-owned источник.

Derived context должен хранить `source_refs[]`, образуя DAG происхождения поверх существующей линейной receipt chain.

Для производного элемента сохраняются непосредственные `source_refs`; полный
DAG восстанавливается рекурсивным разрешением этих ссылок до исходных узлов.
Транзитивную копию всех ссылок в каждом summary не делать: это дублирует
контекст и быстрее исчерпывает `MAX_SOURCE_REFS_PER_ENTRY`. Resolver обязан
проверять циклы, глубину и суммарные лимиты на каждом переходе.

Пример:

```text
workspace chunk ─┐
memory entry ────┼─> summary ──> model request
child report ────┘
```

Receipt chain остаётся линейной. Provenance graph не заменяет её.

## Workspace evidence

Для выбранного фрагмента сохранять минимум:

```text
canonical path
content hash
selected range/chunk identity
captured content или immutable artifact ref
```

Путь сам по себе недостаточен. После capture файл может измениться.

Snapshot operation должна согласованно получить bytes, hash и metadata. Core
читает bytes через один открытый snapshot/handle, вычисляет hash именно этих
bytes и получает metadata из того же snapshot; до commit выполняется проверка
identity/mtime/size и при гонке чтение повторяется. Captured artifact и его
metadata фиксируются до или в той же транзакционной операции, что и source
row. Именно captured bytes/projection участвуют в request; текущее содержимое
пути после capture никогда не используется для реконструкции.

Captured bytes подчиняются тем же правилам удаления, что и остальной model-visible текст, см. [05.8](05-8-redaction-and-retention.md).

## Memory evidence

Исторический request должен ссылаться на точную revision/version:

```text
memory_id
revision
content_hash
```

Supersede не меняет факт того, что модель видела старую revision: envelope ссылается на зафиксированную revision, а не на «текущую». UI/export по-прежнему применяют текущую privacy/redaction policy.

`forget` — другой случай, см. [05.8](05-8-redaction-and-retention.md): provenance не имеет права стать вторым хранилищем стёртого текста.

При `forget` memory `source_hash` остаётся digest, но payload и доступ к
стёртому тексту не сохраняются. При ambient-удалении и `forget_window`
`source_hash` удаляется, а затронутый envelope переходит в явное
`REQUEST_REDACTED`-состояние по правилам 05.8; это не повод возвращать текущую
revision памяти или маскировать redaction под `REQUEST_SOURCE_MISSING`.

## Child evidence

Для child report сохранять:

```text
child_task_id
child_revision
report_hash
parent_sequence
```

Parent request не должен восстанавливаться из «последнего» child report; он должен ссылаться на exact accepted revision.

Для `plan_review` и `tool_result` действует то же правило: ссылка указывает
на immutable reviewed revision или зафиксированный result artifact, а не на
текущий файл плана или повторный запуск инструмента.

## Разрешение и commit validation

Перед commit Core разрешает каждый `source_ref` через immutable source/artifact
registry и проверяет `source_kind`, `source_id`, `source_version` и
`source_hash`. Сначала должен существовать захваченный immutable source state;
запись envelope, `model_request_sources` и block refs затем фиксируется
атомарно по правилам 05.2. Forward reference, неизвестная revision,
несовпадающий hash, цикл или превышение
`MAX_EVIDENCE_REFS`/`MAX_SOURCE_REFS_PER_ENTRY` блокируют commit типизированной
ошибкой и не допускают provider dispatch.

При реконструкции используются только сохранённые envelope, captured blocks и
immutable artifacts. Resolver не читает текущее workspace, memory или child
task как замену историческому source state. `REQUEST_SOURCE_MISSING` может
быть возвращён только для переходной pre-05.6 записи либо действительно
отсутствующего immutable source state. После redaction/retention используются
`REQUEST_REDACTED`/`REQUEST_RETENTION_PRUNED`; ни одно из этих состояний не
является успешной реконструкцией или новым dispatchable state.

## Тесты

### Unit

- provenance graph validation: цикл и висячая ссылка отвергаются при commit;
  исторический `REQUEST_SOURCE_MISSING` допускается только для переходной
  pre-05.6 записи или разрешённого удаления и не маскируется под успешную
  реконструкцию;
- workspace captured evidence: hash соответствует captured bytes;
- memory revision references: ссылка на revision, а не на «текущую» запись;
- mapping `ContextEvidenceRef` на `model_request_sources` и отсутствие
  дублирующего списка контекста;
- classification/projection сохраняются в envelope и проходят redaction/
  retention policy;
- превышение `MAX_EVIDENCE_REFS` и `MAX_SOURCE_REFS_PER_ENTRY` даёт типизированную ошибку.

### Integration

1. **File mutation:** файл меняется после capture, historical request восстанавливает старый captured content.
2. **Child:** parent request указывает exact accepted child revision.
3. **Memory supersede:** после замены записи исторический envelope продолжает ссылаться на старую revision.
4. **Reconstruction:** исторический request восстанавливается только из
   captured immutable state; изменение текущего workspace, memory или child
   report не меняет результат.
5. **Workspace snapshot race:** изменение файла во время capture либо приводит
   к повторному согласованному snapshot, либо к отказу commit; bytes/hash/
   metadata не расходятся.

## Критерии готовности

1. Каждый derived context item имеет непустой `source_refs`.
2. Required refs разрешаются только в уже существующий immutable state.
3. Реконструкция исторического запроса не читает текущее состояние workspace, memory или child task.
4. Каждый `ContextEvidenceRef` однозначно отображается на
   `model_request_sources`, а `classification` и `projection` не создают
   второго независимого описания контекста.
5. `source_hash` соответствует правилам 05.8: для `forget` памяти сохраняется
   digest, для ambient-удаления и `forget_window` удаляется.
6. `source_version`, типы `kind`, разрешение ссылок, лимиты и атомарность
   workspace snapshot явно определены и покрыты тестами.
