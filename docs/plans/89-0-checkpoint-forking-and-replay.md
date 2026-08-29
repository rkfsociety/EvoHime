# План 89.0 — Checkpoint Forking & Replay: branch-and-compare запусков из сохранённого состояния

Статус: предложено по [issue #69](https://github.com/rkfsociety/EvoHime/issues/69). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Расширить реализованный Task Checkpoint/recovery механизм отдельной возможностью **Fork & Replay**: создавать новый run lineage из выбранного сохранённого checkpoint или task boundary, не изменяя исходную историю выполнения.

Пользователь или eval/runtime должен иметь возможность:

- повторить задачу с определённого шага;
- изменить разрешённые входы/модель/strategy profile;
- попробовать альтернативный план;
- исправить downstream этап без повторного выполнения дорогих upstream шагов;
- сравнить две ветки выполнения;
- сохранить исходный run как immutable reference.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/checkpoint_forking_and_replay.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./89-1-checkpoint-forking-and-replay.md)
- [Этап 2 — runtime-интеграция и recovery](./89-2-checkpoint-forking-and-replay.md)
- [Этап 3 — IPC, client projection и UI](./89-3-checkpoint-forking-and-replay.md)
- [Этап 4 — verification, release-evidence и закрытие](./89-4-checkpoint-forking-and-replay.md)

## Зависимости

### Блокирующие

- План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake.
- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- План 58.0 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 73.0 — Dependency-Aware Task Graph: selective replanning и downstream invalidation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- fork создаёт новую run identity;
- grants revalidated;
- credentials не копируются raw;
- side effects не replay-ятся автоматически;
- workspace rollback не destructive;
- source checkpoint immutable;
- branch cannot mutate parent history;
- imported branch metadata не становится trusted без validation;
- Secret projections соблюдают policy новой ветки.

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

- [ ] Можно создать новый immutable lineage из checkpoint.
- [ ] Resume и Fork имеют разные semantics.
- [ ] Replay from task/step использует validated boundary.
- [ ] Workspace drift проверяется явно.
- [ ] External effects не повторяются автоматически.
- [ ] Overrides ограничены обычными Core policies.
- [ ] Есть branch lineage/comparison projection.
- [ ] Branch survives restart независимо от parent.

## Ограничения и non-goals

- destructive rollback пользовательского workspace;
- Git branch management как обязательная реализация;
- автоматический merge лучших model outputs;
- повтор всех network/write actions при replay;
- клонирование raw credentials;
- бесконечное дерево веток без retention/budget limits.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#69 Checkpoint Forking & Replay: branch-and-compare запусков из сохранённого состояния](https://github.com/rkfsociety/EvoHime/issues/69)
