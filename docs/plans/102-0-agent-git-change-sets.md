# План 102.0 — Agent Git Change Sets: безопасные commit-кандидаты, attribution и undo boundary

Статус: предложено по [issue #82](https://github.com/rkfsociety/EvoHime/issues/82). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Agent Git Change Sets**: Core-owned слой, который связывает фактический `WorkspaceChangeSet` конкретного agent run с безопасным Git commit candidate, отделяет изменения агента от уже существовавших user/external dirty changes и предоставляет понятные Commit/Undo/Keep operations.

Это не означает автоматический commit после каждого ответа. Git publication остаётся policy/user-controlled.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/agent-git-change-sets.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 61.0 — Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач.
- План 78.0 — Capability Workbenches: lifecycle-scoped tool groups with shared state and resources.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 82.0 — Context Mentions: typed @references для files, folders, git, diagnostics и runtime resources.
- План 89.0 — Checkpoint Forking & Replay: branch-and-compare запусков из сохранённого состояния.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- no automatic `git add -A`;
- pre-existing staged/dirty changes excluded by default;
- candidate pinned к exact HEAD + diff hash;
- secret/local config excluded by policy;
- commit cannot include path outside granted workspace;
- model cannot choose arbitrary Git author identity;
- hooks проходят ExecutionPolicy;
- no force reset/rebase/push as part of normal commit operation;
- undo preserves unrelated user changes;
- attribution uncertainty is explicit.

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

- [ ] Agent change set связан с exact Git/workspace baseline.
- [ ] Pre-existing/user/external changes классифицируются отдельно.
- [ ] Commit candidate содержит explicit included diff и stale preconditions.
- [ ] Shared staging state не может незаметно добавить unrelated changes.
- [ ] Есть preview/message/commit flow и safe undo semantics.
- [ ] Attribution configurable и не подменяет user Git identity молча.
- [ ] Task Worktree/Incremental Change integrations используют тот же contract.

## Ограничения и non-goals

- automatic commit после каждого model turn;
- automatic push;
- force-reset/rebase;
- обязательное изменение author identity;
- идеальная hunk attribution после массовых formatter rewrites;
- замена Workspace Checkpoints/Git;
- commit всех dirty files ради удобства.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#82 Agent Git Change Sets: безопасные commit-кандидаты, attribution и undo boundary](https://github.com/rkfsociety/EvoHime/issues/82)
