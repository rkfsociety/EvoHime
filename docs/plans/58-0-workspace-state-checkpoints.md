# План 58.0 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history

Статус: предложено по [issue #38](https://github.com/rkfsociety/EvoHime/issues/38). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный **Workspace State Checkpoint**: локальный snapshot состояния рабочих файлов, который позволяет сравнить и восстановить изменения агента независимо от semantic `TaskCheckpoint` и истории conversation.

Ключевой принцип:

> состояние задачи и состояние файлов — разные вещи.

Существующий/запланированный `TaskCheckpoint` отвечает на вопросы «что сделано, что осталось, какие решения и blockers активны». Новый Workspace Checkpoint отвечает на вопрос «какое состояние файлов было в конкретной точке и как безопасно к нему вернуться».

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/workspace-state-checkpoints.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./58-1-workspace-state-checkpoints.md)
- [Этап 2 — runtime-интеграция и recovery](./58-2-workspace-state-checkpoints.md)
- [Этап 3 — IPC, client projection и UI](./58-3-workspace-state-checkpoints.md)
- [Этап 4 — verification, release-evidence и закрытие](./58-4-workspace-state-checkpoints.md)

## Зависимости

### Блокирующие

- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- snapshot path resolution Core-owned;
- запрещён traversal/reparse escape;
- пользовательский `.git` не модифицируется ради checkpoint;
- restore не расширяет filesystem grants;
- external/user changes не overwrite-ятся молча;
- Secret blobs обрабатываются по sensitivity policy;
- checkpoint не считается способом откатить внешние side effects.

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

- [ ] Workspace checkpoint отделён от TaskCheckpoint.
- [ ] Snapshot backend не загрязняет пользовательский Git history.
- [ ] Можно сравнить checkpoint с текущим workspace.
- [ ] Есть независимые RestoreWorkspace/RestoreTask/RestoreBoth flows.
- [ ] Preflight защищает user/external changes от тихого overwrite.
- [ ] Snapshot storage content-addressed/bounded и sensitivity-aware.
- [ ] Restore journaled и recoverable.

## Ограничения и non-goals

- полноценная backup-система всего компьютера;
- откат внешних API/network side effects;
- автоматический force reset пользовательского Git;
- checkpoint каждого файла после каждого token;
- хранение build caches/dependencies целиком;
- замена Git commits/branches/worktrees.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#38 Workspace State Checkpoints: безопасный rollback файлов отдельно от task history](https://github.com/rkfsociety/EvoHime/issues/38)
