# План 31.2 — Visual Workflow Builder: typed canvas, validation и live runtime inspection: authoring-интеграция, read-only inspection и recovery draft

Статус: этап 2 для [плана 31.0](./31-0-visual-workflow-builder.md); после [плана 31.1](./31-1-visual-workflow-builder.md).

## Цель

Реализовать Core-owned authoring и read-only inspection: draft edit -> validation -> conflict-safe save -> immutable workflow version, а также восстановление draft и чтение snapshot/event state уже существующего runtime. Этот этап не запускает workflow и не создаёт новый runtime.

## Зависимости

### Блокирующие

- План 31.1 — contract, validators, storage policy и errors.
- Existing workflow registry/contract, workflow runtime journal, audit и authenticated Core boundary; runtime execution semantics остаются в существующем workflow runtime.

### Опциональные

- План 30.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 contract и принимать только typed draft commands: add/remove/move/connect/update metadata, с bounded payload и optimistic version.
1. Реализовать Core handler/state machine для normalize -> registry bind -> validate -> persist draft -> publish immutable definition; повторить authorization перед сохранением.
2. Разделить execution hash и layout hash и доказать, что layout edit не меняет graph semantics. Запрещать mutation published definition и любого running snapshot.
3. Реализовать recovery draft: после restart восстановить только последний валидный draft/revision либо typed conflict/corrupt outcome; не превращать незавершённый draft в published workflow.
4. Реализовать read-only projection существующего `workflow_runtime`/event journal: состояния узлов, sequence/replay и redacted refs без повторного dispatch, retry или изменения run.
5. Сделать fault-injection для crash во время draft save, stale revision, duplicate command, registry/policy change и corruption; зафиксировать metadata-only evidence для этапа 3.

## Предметная декомпозиция

### Authoring and inspection vertical slice

- Entrypoint: `crates/evohime-core/src/visual_workflow_builder.rs` + command handler в `crates/evohime-core/src/ipc_bridge.rs`; сервис выполняет только `validate → policy → bounded draft mutation → typed result/event`.
- Для публикации сохранить immutable definition и execution hash; для инспекции читать только уже созданные runtime snapshot/events.
- Тесты: `crates/evohime-core/tests/visual_workflow_builder_recovery.rs` — draft crash/restart, duplicate, stale revision, invalid binding, corruption, published/running immutability и replay.

### Acceptance-to-authoring/inspection matrix

- `C01` — Есть canvas над существующим workflow contract. → провести через typed draft mutations, validation и idempotency.
- `C05` — Layout metadata отделена от execution hash. → доказать независимыми hashes и fixture с перемещением узла.
- `C06` — Есть recovery draft. → журналировать draft transitions и восстановление без публикации/dispatch.

### Recovery contract

- Durable draft transitions восстанавливаются replay/reconciliation; повреждённый или stale draft получает typed `corrupt`/`conflict`, а не публикуется молча.
- Fault injection должна доказать отсутствие частичной записи, изменения published/running graph, обход policy или расширения capability set.

## Критерии выхода

- [ ] Happy path draft mutation/publish выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/corrupt/restart имеют отдельные outcomes.
- [ ] Published definition и active run snapshot не изменяются Builder-ом.
- [ ] Live inspection read-only и pinned к фактическому runtime snapshot/event sequence.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
