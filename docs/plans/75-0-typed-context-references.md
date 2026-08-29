# План 75.0 — Typed Context References: адресные @refs на файлы, diff, diagnostics, terminal и artifacts

Статус: предложено по [issue #55](https://github.com/rkfsociety/EvoHime/issues/55). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Typed Context References**: единый механизм, позволяющий пользователю и agent-facing UI адресно прикладывать к сообщению конкретный файл, папку, diff, diagnostics, terminal range, commit, artifact, task/plan или другой зарегистрированный ресурс без ручного копирования содержимого в prompt.

Главный принцип:

> `@что-то` должно превращаться не в строковую магию, а в typed reference, который Core разрешает, проверяет, ограничивает по размеру и только затем строит безопасную model projection.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/typed_context_references.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./75-1-typed-context-references.md)
- [Этап 2 — runtime-интеграция и recovery](./75-2-typed-context-references.md)
- [Этап 3 — IPC, client projection и UI](./75-3-typed-context-references.md)
- [Этап 4 — verification, release-evidence и закрытие](./75-4-typed-context-references.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 56.0 — Artifact Handoff Registry: typed deliverables, lineage и freshness для multi-agent работы.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть versioned ContextRef/ResolvedContextRef contracts.
- [ ] Core имеет typed resolver registry.
- [ ] Поддержаны file/folder/diff/commit/diagnostics/terminal/artifact/task/plan refs.
- [ ] Expansion lazy и bounded контекстным бюджетом.
- [ ] Mutable refs фиксируются на конкретную revision/hash перед model call.
- [ ] `@` autocomplete работает через typed resources.
- [ ] Context refs не расширяют capabilities.
- [ ] Model-call provenance содержит использованные refs/projections.

## Ограничения и non-goals

- вставлять целый repository через один `@folder`;
- превращать `@` syntax в shell/tool language;
- direct renderer filesystem/network access;
- считать содержимое referenced files trusted instructions;
- бесконечно расширять context budget;
- поддерживать произвольные executable third-party resolvers из project files.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#55 Typed Context References: адресные @refs на файлы, diff, diagnostics, terminal и artifacts](https://github.com/rkfsociety/EvoHime/issues/55)
