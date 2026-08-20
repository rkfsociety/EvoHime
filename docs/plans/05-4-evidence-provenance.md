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

- [05.6](05-6-compaction-shadowing.md) — append-only shadowing. До неё summary фиксируется с `source_refs`, но вытесненные из контекста originals могут исчезнуть из активной проекции; provenance-ссылка при этом становится неразрешимой и отдаёт `REQUEST_SOURCE_MISSING`. Это наблюдаемое состояние, а не тихая потеря; полная сохранность originals появляется вместе с 05.6.

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

Derived context должен хранить `source_refs[]`, образуя DAG происхождения поверх существующей линейной receipt chain.

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

Snapshot operation должна согласованно получить bytes, hash и metadata. Именно captured bytes/projection участвуют в request.

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

## Child evidence

Для child report сохранять:

```text
child_task_id
child_revision
report_hash
parent_sequence
```

Parent request не должен восстанавливаться из «последнего» child report; он должен ссылаться на exact accepted revision.

## Тесты

### Unit

- provenance graph validation: цикл и висячая ссылка отвергаются;
- workspace captured evidence: hash соответствует captured bytes;
- memory revision references: ссылка на revision, а не на «текущую» запись;
- превышение `MAX_EVIDENCE_REFS` и `MAX_SOURCE_REFS_PER_ENTRY` даёт типизированную ошибку.

### Integration

1. **File mutation:** файл меняется после capture, historical request восстанавливает старый captured content.
2. **Child:** parent request указывает exact accepted child revision.
3. **Memory supersede:** после замены записи исторический envelope продолжает ссылаться на старую revision.

## Критерии готовности

1. Каждый derived context item имеет непустой `source_refs`.
2. Required refs разрешаются только в уже существующий immutable state.
3. Реконструкция исторического запроса не читает текущее состояние workspace, memory или child task.
