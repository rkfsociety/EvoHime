# План 98.0 — Durable Remote Task Bridge: submit/status/cancel protocol для долгих tool и MCP операций

Статус: предложено по [issue #78](https://github.com/rkfsociety/EvoHime/issues/78). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Durable Remote Task Bridge**: Core-owned протокол для внешних инструментов и интеграций, у которых операция живёт дольше одного обычного tool call и должна переживать model turns, reconnect и restart приложения.

Основной сценарий:

```text
submit(args) -> remote_task_id
status(remote_task_id) -> running | input_required | completed | failed | cancelled
cancel(remote_task_id) -> terminal/actual status
```

После успешного submit дальнейший lifecycle принадлежит Core, а не памяти модели.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/durable-remote-task-bridge.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 63.0 — Composable Termination Conditions: first-class stop policies for agent and team runs.
- План 77.0 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake.
- План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol.
- План 54.0 — Human Work Items: пользователь как полноценный участник workflow/team, а не только approval.
- План 93.0 — Headless CLI Client: NDJSON streaming, one-shot runs и automation поверх существующего Core.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- toolset registry Core-owned;
- status/cancel не обходят capability checks;
- task ID считается opaque data, не shell/URL instruction;
- credentials остаются provider refs;
- result/input payload size-bounded и schema-validated;
- poller не наследует unrestricted parent authority;
- dangerous completed result всё равно проходит обычные downstream approvals перед дальнейшим effect;
- imported workflow не может зарегистрировать произвольный status/cancel executable.

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

- [ ] Есть versioned RemoteTaskToolset/RemoteTaskRecord contracts.
- [ ] Submit/status/cancel lifecycle Core-owned и durable.
- [ ] Pending tasks переживают restart.
- [ ] Polling bounded, leased и backoff-aware.
- [ ] Transport/status-call failure отделён от remote task failure.
- [ ] Results сохраняются как structured data/artifact refs.
- [ ] MCP и Integration Provider могут использовать один bridge.
- [ ] Unknown outcomes не вызывают blind retry side effects.

## Ограничения и non-goals

- универсальный distributed job scheduler;
- polling каждой обычной tool operation;
- хранение remote credentials в task record;
- автоматическое толкование prose как task protocol;
- бесконечный polling без budgets/timeouts;
- замена workflow runtime;
- автоматический retry submit после ambiguous side effect.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#78 Durable Remote Task Bridge: submit/status/cancel protocol для долгих tool и MCP операций](https://github.com/rkfsociety/EvoHime/issues/78)
