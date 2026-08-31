# План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection

Статус: предложено по [issue #40](https://github.com/rkfsociety/EvoHime/issues/40). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Формализовать в EvoHime **Revision-Safe Workspace Files**: Core-owned файловую модель с отдельными namespaces для входных пользовательских файлов, рабочего состояния и результатов, а также optimistic concurrency/precondition checks для всех agent-driven изменений файлов.

Главная задача — исключить класс ошибок, когда агент читает revision A, пользователь/другой agent изменяет файл до revision B, а первый агент затем молча перезаписывает B результатом, построенным на устаревшем состоянии.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

План обязан стать общей mutation boundary для существующих
`filesystem.read`, `filesystem.write`, `filesystem.patch`, advanced file tools
и mediated external-agent writes. Сейчас read возвращает host path, write не
имеет precondition, а patch допускает fuzzy context search; эти legacy paths
должны мигрировать на один Core service, иначе новый contract не считается
реализованным. ArtifactStore владеет upload/output bytes, sandbox — canonical
path containment, event journal — mutation intent/outcome.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./60-1-revision-safe-workspace-files.md)
- [Этап 2 — runtime-интеграция и recovery](./60-2-revision-safe-workspace-files.md)
- [Этап 3 — IPC, client projection и UI](./60-3-revision-safe-workspace-files.md)
- [Этап 4 — verification, release-evidence и закрытие](./60-4-revision-safe-workspace-files.md)

## Зависимости

### Блокирующие

- существующие ArtifactStore, tool registry, workspace sandbox/path
  canonicalization, event journal и Sensitive Data Guardrails v1;
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Incremental Change Protocol (план 59) может передавать plan-item/change-set
  provenance; без него file mutations сохраняют run/tool provenance.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Логически разделить минимум:

```text
uploads/     # immutable/user-provided inputs или imported attachments
workspace/   # project files, которые можно читать/изменять согласно grants
outputs/     # generated/exportable deliverables
scratch/     # run-scoped ephemeral working files
```

Это не обязательно означает физически четыре соседние папки в каждом проекте. Это должен быть **typed namespace contract**, который Core проецирует на конкретный storage/backend.

### Безопасность

- model не получает unrestricted absolute host paths;
- write только внутри granted namespace/path scope;
- uploads immutable by default;
- stale precondition fails closed;
- scratch isolated per run;
- outputs не применяются обратно в workspace автоматически;
- traversal/reparse escape blocked;
- sensitivity metadata сохраняется при копировании/derived output;
- external agents работают через тот же revision contract, если Core может посредничать их file writes.

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

- [ ] Есть typed namespaces uploads/workspace/outputs/scratch.
- [ ] File refs несут content hash/revision.
- [ ] Mutations поддерживают expected hash/revision preconditions.
- [ ] Stale write никогда не применяется молча.
- [ ] Uploads immutable по умолчанию, scratch run-scoped.
- [ ] После изменений создаётся observed WorkspaceChangeSet.
- [ ] External edits инвалидируют stale refs.
- [ ] Path traversal/symlink/reparse escape закрыты.
- [ ] Workbench/Artifact layers используют refs, а не обходят Core file semantics.

## Ограничения и non-goals

- замена Git системой собственных файловых revisions;
- distributed filesystem;
- автоматический semantic merge любых конфликтов;
- хранение копии каждого repository revision;
- direct renderer filesystem authority;
- silent last-write-wins;
- автоматическое выполнение/открытие generated outputs.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#40 Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection](https://github.com/rkfsociety/EvoHime/issues/40)

## Результат ревью 2026-09-01

- План привязан к фактическим legacy read/write/patch paths и требует их
  миграции на единую boundary, включая отказ от default fuzzy patch apply.
- Persistence разделена по owners: ArtifactStore для upload/output bytes,
  event journal для mutation recovery, on-demand refs для workspace и
  run-scoped cleanup для scratch.
