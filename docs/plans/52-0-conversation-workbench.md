# План 52.0 — Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage

Статус: предложено по [issue #32](https://github.com/rkfsociety/EvoHime/issues/32). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Conversation Workbench**: единую контекстную рабочую поверхность вокруг активной conversation/run, где пользователь может инспектировать файлы, изменения, задачи, terminal, browser state, usage и другие runtime projections, не покидая основной диалог.

Workbench не должен становиться ещё одним runtime или источником полномочий. Это renderer/presentation слой над Core-owned services и Resumable Conversation Event Log.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/conversation_workbench.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./52-1-conversation-workbench.md)
- [Этап 2 — runtime-интеграция и recovery](./52-2-conversation-workbench.md)
- [Этап 3 — IPC, client projection и UI](./52-3-conversation-workbench.md)
- [Этап 4 — verification, release-evidence и закрытие](./52-4-conversation-workbench.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Conversation
  + Workbench tabs
      Files
      Diff
      Tasks
      Terminal
      Browser
      Usage
      Plan/Artifacts (optional)
```

Все tabs привязаны к одному `conversation_id`, workspace/run snapshot и backend capability set.

### Безопасность

- renderer не получает direct filesystem/shell/browser authority;
- file paths canonicalized Core-side;
- tab availability следует capability snapshot;
- Secret payload masked/not projected;
- terminal manual input не обходит ExecutionPolicy;
- browser URL не превращается автоматически в executable navigation capability;
- related refs typed и validated;
- disabled capability нельзя вызвать через скрытый tab/API;
- cached UI state не хранит raw credentials/secrets.

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

- [ ] Есть единый Conversation Workbench рядом с chat.
- [ ] Files/Diff/Tasks/Terminal/Browser/Usage представлены отдельными capability-aware tabs.
- [ ] Все authoritative операции проходят Core services.
- [ ] Tabs scoped к текущей conversation/workspace/backend snapshot.
- [ ] Есть typed cross-links из conversation events в workbench resources.
- [ ] Presentation state безопасно сохраняется per conversation.
- [ ] Live updates используют общий event/projection механизм.
- [ ] Sensitive data и unavailable capabilities корректно ограничены.

## Ограничения и non-goals

- полноценная IDE вместо VS Code/другого editor;
- direct renderer filesystem access;
- unrestricted embedded browser debugging interface;
- второй workflow/task database для UI;
- обязательный terminal PTY, если backend его не поддерживает;
- выполнение arbitrary commands через вкладку без Core policy;
- одинаковый набор tabs на backend, которые не имеют соответствующих capabilities.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#32 Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage](https://github.com/rkfsociety/EvoHime/issues/32)
