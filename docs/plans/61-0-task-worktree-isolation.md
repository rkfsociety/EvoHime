# План 61.0 — Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач

Статус: предложено по [issue #41](https://github.com/rkfsociety/EvoHime/issues/41). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Task Worktree Isolation**: возможность запускать отдельную coding-задачу, child role или параллельную ветку работы в собственном Git worktree и branch, чтобы несколько agent runs не конкурировали за один mutable working tree.

Это не новый Git-клиент и не замена workflow runtime. Worktree становится **workspace backend конкретного run**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/task-worktree-isolation.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./61-1-task-worktree-isolation.md)
- [Этап 2 — runtime-интеграция и recovery](./61-2-task-worktree-isolation.md)
- [Этап 3 — IPC, client projection и UI](./61-3-task-worktree-isolation.md)
- [Этап 4 — verification, release-evidence и закрытие](./61-4-task-worktree-isolation.md)

## Зависимости

### Блокирующие

- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake.
- План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Primary Workspace
  repo HEAD=A

Task 1 -> worktree/feature-1 -> branch eva/task-1
Task 2 -> worktree/feature-2 -> branch eva/task-2
Reviewer -> read-only snapshot/worktree for Task 1
```

Каждый run фиксирует свой workspace root и не использует primary checkout неявно.

### Безопасность

- root path Core-created/canonicalized;
- worktree не расширяет repo/filesystem grants;
- auxiliary files allowlisted и secret-safe;
- model не выбирает arbitrary destination path;
- merge/apply требует target fingerprint preflight;
- dirty user changes не overwrite-ятся;
- cleanup не удаляет unintegrated state без policy;
- imported workflow не может указать host path для worktree;
- branch/ref validation защищает от ref injection.

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

- [ ] Core имеет durable TaskWorktree registry.
- [ ] Runs/children могут быть pinned к isolated worktree root.
- [ ] Parallel agents не используют один mutable checkout по умолчанию при включённой isolation policy.
- [ ] Есть explicit integration actions с preflight/conflict semantics.
- [ ] Auxiliary files копируются только по безопасной policy.
- [ ] Worktree lifecycle/recovery/cleanup не теряет unintegrated changes.
- [ ] WorkspaceCheckpoints поддерживаются per worktree.

## Ограничения и non-goals

- собственная замена Git;
- автоматическое разрешение любых merge conflicts моделью без review;
- force push/reset как default integration;
- копирование всех ignored files;
- отдельная полная clone на каждый child;
- скрытый merge результатов всех агентов в primary workspace.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#41 Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач](https://github.com/rkfsociety/EvoHime/issues/41)
