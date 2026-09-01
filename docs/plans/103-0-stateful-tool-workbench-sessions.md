# План 103.0 — Stateful Tool Workbench Sessions: lifecycle, shared state и snapshot для tool collections

Статус: предложено по [issue #83](https://github.com/rkfsociety/EvoHime/issues/83). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Stateful Tool Workbench Session**: Core-owned abstraction для наборов tools, которым нужен общий lifecycle и временное состояние между несколькими tool calls внутри одного run/session.

Это особенно полезно для:

- MCP servers;
- browser/terminal-like tool collections;
- notebook/analysis backends;
- remote tool hosts;
- stateful SDK adapters.

Workbench не заменяет Tool Registry и не даёт модели прямого доступа к процессу/server transport. Он управляет жизненным циклом зарегистрированной коллекции tools.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/stateful_tool_workbench_sessions.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./103-1-stateful-tool-workbench-sessions.md)
- [Этап 2 — runtime-интеграция и recovery](./103-2-stateful-tool-workbench-sessions.md)
- [Этап 3 — IPC, client projection и UI](./103-3-stateful-tool-workbench-sessions.md)
- [Этап 4 — verification, release-evidence и закрытие](./103-4-stateful-tool-workbench-sessions.md)

## Зависимости

### Блокирующие

- План 78.0 — Capability Workbenches: lifecycle-scoped tool groups with shared state and resources.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 38.0 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas.
- Tool Simulation Runtime v1 из `../architecture.md`.
- Канонический раздел `architecture.md` — Agentic Browser Session v1 (бывший план 55).
- Composable Termination Conditions v1 — реализованный Core-контракт из канонических документов.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Created
 -> Starting
 -> Ready
 -> Active
 -> Closing
 -> Closed
```

Ошибки:

```text
StartFailed
ConnectionLost
BackendCrashed
InvalidState
Expired
```

Core должен явно различать backend crash и обычный tool error.

### Безопасность

- workbench identities Core-owned;
- backend-advertised tools проходят registry validation;
- session scope bounded;
- private backend state не попадает модели;
- snapshots sensitivity-aware и optional;
- secrets не сериализуются в state snapshot;
- process adapters используют ExecutionPolicy;
- stale refs после reset/restart rejected;
- imported workflow не может зарегистрировать executable workbench adapter.

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

- [ ] Есть versioned WorkbenchDefinition и runtime session.
- [ ] Stateful tool collections имеют explicit lifecycle.
- [ ] Session scope и concurrency policy формализованы.
- [ ] Есть reset/restart/close semantics.
- [ ] Optional snapshot/restore безопасно ограничены adapter capability.
- [ ] MCP/stateful backends могут переиспользовать один lifecycle contract.
- [ ] Backend private state не становится model authority.

## Ограничения и non-goals

- глобальные бессрочные tool processes;
- сериализация произвольной process memory;
- raw socket/handle exposure модели;
- автоматическая регистрация tools от любого подключённого server;
- хранение credentials в workbench snapshot;
- замена Integration Provider SDK или Tool Registry.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#83 Stateful Tool Workbench Sessions: lifecycle, shared state и snapshot для tool collections](https://github.com/rkfsociety/EvoHime/issues/83)
