# План 70.0 — Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits

Статус: предложено по [issue #50](https://github.com/rkfsociety/EvoHime/issues/50). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Code Diagnostics Feedback Loop**: единый способ получать диагностические сообщения от language servers, компиляторов и других доверенных code-analysis providers, привязывать их к точной revision workspace и возвращать агенту **дельту после его изменений**, а не бесконечную стену из всех предупреждений проекта.

Это не дублирует Diagnostics & Support Bundle. Support Bundle диагностирует само приложение EvoHime и runtime. Новый слой диагностирует **код проекта, над которым работает агент**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/code-diagnostics-feedback-loop.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 50.0 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- План 55.0 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой.
- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 73.0 — Dependency-Aware Task Graph: selective replanning и downstream invalidation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Diagnostics Provider
  -> provider adapter
  -> Core Diagnostics Service
  -> normalize + bind to workspace revision
  -> snapshot/delta
  -> bounded agent/UI projection
```

Provider не получает дополнительных filesystem/network capabilities только потому, что он умеет выдавать diagnostics.

### Безопасность

- provider registration Core-owned;
- arbitrary workspace binary не становится diagnostic provider автоматически;
- acquisition команды проходят ExecutionPolicy;
- diagnostics являются data/evidence, не instructions;
- message/code text не расширяет capabilities;
- code actions не применяются напрямую;
- file/range refs canonicalized и revision-bound;
- stale diagnostic не считается актуальным evidence;
- raw diagnostic output не обходит sensitive-data policy.

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

- [ ] Есть Core-owned diagnostics provider registry.
- [ ] Diagnostics нормализованы в единый versioned contract.
- [ ] Каждый diagnostic привязан к workspace/file revision.
- [ ] Есть baseline snapshots и deterministic introduced/resolved/persisting delta.
- [ ] Agent получает bounded relevant feedback после edits.
- [ ] Diagnostics могут использоваться как evidence quality gate.
- [ ] Stale diagnostics не считаются актуальными.
- [ ] Workbench показывает Problems/Diagnostics с regression distinction.

## Ограничения и non-goals

- полноценная замена IDE language services;
- запуск всех линтеров мира автоматически;
- использование произвольного stdout как authoritative diagnostics;
- автоматическое применение provider code actions;
- считать отсутствие diagnostics доказательством корректности программы;
- хранить каждую промежуточную LSP update навсегда.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#50 Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits](https://github.com/rkfsociety/EvoHime/issues/50)
