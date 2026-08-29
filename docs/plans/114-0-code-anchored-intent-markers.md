# План 114.0 — Code-Anchored Intent Markers: задачи и вопросы Еве прямо из комментариев в исходниках

Статус: предложено по [issue #94](https://github.com/rkfsociety/EvoHime/issues/94). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Code-Anchored Intent Markers**: безопасный coding UX, позволяющий пользователю оставить задачу или вопрос непосредственно в комментарии исходного файла, после чего Core обнаруживает marker, привязывает его к точной revision/range/symbol context и предлагает создать соответствующий agent task.

Пример UX:

```rust
// EVA! вынеси эту проверку в отдельную функцию и добавь тест
```

или:

```ts
// EVA? почему здесь нужен второй debounce?
```

Синтаксис конкретных префиксов можно скорректировать при реализации. Важно не само слово `EVA`, а typed semantics двух базовых intent kinds:

```text
EditRequest
Question
```

Это не generic file-trigger automation и не выполнение команд из комментариев. Marker является удобной **точкой постановки coding-задачи, привязанной к коду**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/code_anchored_intent_markers.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./114-1-code-anchored-intent-markers.md)
- [Этап 2 — runtime-интеграция и recovery](./114-2-code-anchored-intent-markers.md)
- [Этап 3 — IPC, client projection и UI](./114-3-code-anchored-intent-markers.md)
- [Этап 4 — verification, release-evidence и закрытие](./114-4-code-anchored-intent-markers.md)

## Зависимости

### Блокирующие

- План 70.0 — Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits.
- План 73.0 — Dependency-Aware Task Graph: selective replanning и downstream invalidation.
- План 75.0 — Typed Context References: адресные @refs на файлы, diff, diagnostics, terminal и artifacts.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- План 82.0 — Context Mentions: typed @references для files, folders, git, diagnostics и runtime resources.
- План 97.0 — Model Edit Protocol Registry: строгие patch/search-replace стратегии и repair feedback.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Workspace watcher / explicit scan
  -> candidate comment detection
  -> language-aware marker parser
  -> authorship/provenance classification
  -> revision/range binding
  -> dedup + stale checks
  -> CodeIntentMarker
  -> user-visible task proposal
  -> normal EvoHime agent/run path
```

Marker никогда не вызывает shell/file/network tool напрямую.

Все последующие действия идут через обычные:

- capabilities/grants;
- approvals;
- Revision-Safe Workspace Files;
- model/tool runtime;
- diagnostics/quality gates.

### Безопасность

- marker является user intent only после trusted provenance classification;
- existing repository comments inert by default;
- agent-generated markers не auto-trigger;
- marker никогда не является capability/approval grant;
- Question mode read-oriented by default;
- all writes проходят Revision-Safe Workspace Files;
- paths/ranges canonicalized и revision-bound;
- project/dependency text не получает instruction authority автоматически;
- comment parser не исполняет embedded code;
- marker rate/debounce bounded;
- recursive self-trigger blocked;
- sensitive source projection следует обычным guardrails;
- auto-start profile заранее Core-configured и immutable для marker text.

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

- [ ] Есть typed CodeIntentMarker contract/lifecycle.
- [ ] Поддержаны Question и EditRequest markers.
- [ ] Parsing выполняется по comment ranges, а не raw whole-file regex в auto mode.
- [ ] Marker привязан к exact file revision/range и optional semantic symbol.
- [ ] Есть trusted authorship/provenance classification.
- [ ] Existing/agent-generated content не auto-trigger-ится.
- [ ] Есть debounce/dedup/stale/loop protection.
- [ ] Marker запускает обычный EvoHime task, не отдельный небезопасный runtime.
- [ ] Workbench/conversation умеет перейти marker -> run -> diff/result.

## Ограничения и non-goals

- выполнение arbitrary команд из комментариев;
- автоматическая активация markers из cloned/imported repository;
- IDE replacement;
- отдельный programming language внутри comments;
- автоматическое удаление markers без видимого diff;
- выдача grants/approvals через marker text;
- рекурсивное создание задач самим агентом через новые markers.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#94 Code-Anchored Intent Markers: задачи и вопросы Еве прямо из комментариев в исходниках](https://github.com/rkfsociety/EvoHime/issues/94)
