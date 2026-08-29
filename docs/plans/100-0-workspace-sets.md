# План 100.0 — Workspace Sets: multi-root и cross-repository задачи с независимыми grants, VCS и checkpoints

Статус: предложено по [issue #80](https://github.com/rkfsociety/EvoHime/issues/80). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Workspace Set**: Core-owned объединение нескольких workspace roots/repositories в одну task/conversation context boundary для задач, которые затрагивают frontend, backend, contracts, services или другие связанные проекты одновременно.

Каждый root остаётся самостоятельным security/VCS namespace. Workspace Set даёт агенту возможность координировать cross-repository работу, но не превращает несколько директорий в один воображаемый atomic filesystem.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/workspace_sets.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./100-1-workspace-sets.md)
- [Этап 2 — runtime-интеграция и recovery](./100-2-workspace-sets.md)
- [Этап 3 — IPC, client projection и UI](./100-3-workspace-sets.md)
- [Этап 4 — verification, release-evidence и закрытие](./100-4-workspace-sets.md)

## Зависимости

### Блокирующие

- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 61.0 — Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 58.0 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history.
- План 73.0 — Dependency-Aware Task Graph: selective replanning и downstream invalidation.
- План 89.0 — Checkpoint Forking & Replay: branch-and-compare запусков из сохранённого состояния.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- каждый root имеет отдельный grant boundary;
- path refs root-qualified и canonicalized;
- parent directory не становится implicit workspace;
- shell cwd не выходит за selected root без отдельного grant;
- adding root does not mutate active run permissions;
- rules/skills scoped к source root;
- partial multi-root outcomes reported explicitly;
- restore/merge не force-ит независимые repos;
- Secret/sensitive roots можно полностью исключить из model context;
- imported set config не создаёт arbitrary host path binding без user mapping.

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

- [ ] Есть durable/versioned WorkspaceSet с уникальными root aliases.
- [ ] Каждый root имеет отдельные grants, VCS и revision identity.
- [ ] Context/files/commands используют root-qualified refs.
- [ ] Cross-root search/edit/tasks работают без implicit parent-directory authority.
- [ ] Rules, Skills, Diagnostics и Checkpoints корректно scoped per root.
- [ ] Multi-root checkpoints/worktree bindings имеют coordinated, но честно non-atomic semantics.
- [ ] Partial outcomes/recovery фиксируются per root.
- [ ] Active run pinned к exact set/root binding snapshot.

## Ограничения и non-goals

- distributed filesystem;
- настоящие ACID transactions между Git repositories;
- автоматический clone всех missing roots;
- force merge/reset нескольких repos ради единого результата;
- implicit access ко всему parent directory;
- единый Git history для независимых repositories;
- SaaS organization/project workspace management.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#80 Workspace Sets: multi-root и cross-repository задачи с независимыми grants, VCS и checkpoints](https://github.com/rkfsociety/EvoHime/issues/80)
