# План 44.0 — Tool Simulation Runtime: fixture/emulated dry-run без реальных side effects

Статус: предложено по [issue #24](https://github.com/rkfsociety/EvoHime/issues/24). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Tool Simulation Runtime**: режим, в котором выбранные tool calls перехватываются до реального исполнения и получают контролируемый synthetic/fixture result. Это нужно для evals, workflow preview, composer validation и безопасного dry-run side-effecting автоматизаций.

Simulation должна быть отдельной runtime capability с явной provenance. Симулированный результат никогда не должен выглядеть как подтверждённый реальный effect.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/tool_simulation_runtime.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./44-1-tool-simulation-runtime.md)
- [Этап 2 — runtime-интеграция и recovery](./44-2-tool-simulation-runtime.md)
- [Этап 3 — IPC, client projection и UI](./44-3-tool-simulation-runtime.md)
- [Этап 4 — verification, release-evidence и закрытие](./44-4-tool-simulation-runtime.md)

## Зависимости

### Блокирующие

- Structured Response Contract v1 (см. `docs/architecture.md`)
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- simulation mode не fallback-ится в Real автоматически;
- emulator не получает credentials/tools;
- simulated result schema-validated;
- synthetic evidence маркируется end-to-end;
- real completion criteria не принимают synthetic evidence по умолчанию;
- fixture payload redacted/synthetic;
- stale schema fixture блокируется;
- renderer всегда показывает simulation state;
- imported content не может скрыть/снять simulation badge.

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

- [ ] Tool runtime имеет explicit Real/Fixture/Emulated/DryRun modes.
- [ ] Fixture matching deterministic и schema-versioned.
- [ ] Missing fixture не вызывает real side effect.
- [ ] Emulated output проходит Structured Response validation.
- [ ] Synthetic evidence отличается от real observed evidence.
- [ ] Workflow может исполняться end-to-end в dry-run.
- [ ] Benchmark Matrix может использовать fixture tools.
- [ ] UI/diagnostics невозможно спутать simulation с production execution.

## Ограничения и non-goals

- считать LLM-emulated response правдой о внешней системе;
- автоматически переключаться на real tool при проблеме fixture;
- полноценный digital twin всех integrations;
- запись production sensitive outputs в fixtures без explicit workflow;
- использование simulation для обхода обязательных real validation gates;
- замена unit tests provider adapters.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#24 Tool Simulation Runtime: fixture/emulated dry-run без реальных side effects](https://github.com/rkfsociety/EvoHime/issues/24)
