# План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking

Статус: предложено по [issue #16](https://github.com/rkfsociety/EvoHime/issues/16). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Расширить существующий EvoHime evaluation catalog отдельным **Agent Benchmark Matrix** для model-dependent поведения: запускать versioned challenge suites по нескольким model/strategy profiles, делать несколько попыток, измерять стабильность, стоимость и latency, сохранять baseline и автоматически выделять regressions/improvements.

Существующие deterministic/static eval gates должны остаться быстрым обязательным PR-контуром. Benchmark Matrix является более дорогим stochastic/nightly/manual уровнем, а не заменой текущих тестов.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/agent_benchmark_matrix.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./36-1-agent-benchmark-matrix.md)
- [Этап 2 — runtime-интеграция и recovery](./36-2-agent-benchmark-matrix.md)
- [Этап 3 — IPC, client projection и UI](./36-3-agent-benchmark-matrix.md)
- [Этап 4 — verification, release-evidence и закрытие](./36-4-agent-benchmark-matrix.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

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

- [ ] Existing deterministic evals остаются отдельным быстрым gate.
- [ ] Есть versioned benchmark challenges/suites.
- [ ] Можно запускать matrix по model + agent profiles.
- [ ] Поддерживаются multiple attempts и pass-rate/statistics.
- [ ] Есть baseline и regression comparison.
- [ ] Есть maintain/improve/explore sets.
- [ ] Security regressions являются hard failures.
- [ ] Runs изолированы и parallelism bounded.
- [ ] Reports machine-readable и redacted.
- [ ] Benchmark failure можно превратить в deterministic regression fixture.

## Ограничения и non-goals

- публичный leaderboard;
- гонка моделей ради одного общего score;
- замена unit/integration/security tests;
- обязательный expensive benchmark на каждый commit;
- использование production user data;
- автоматическое принятие нового baseline после ухудшения;
- скрытие cost/variance за усреднённым «quality score».

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#16 Agent Benchmark Matrix: многократные model/strategy evals и regression tracking](https://github.com/rkfsociety/EvoHime/issues/16)
