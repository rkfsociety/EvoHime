# План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol

Статус: предложено по [issue #25](https://github.com/rkfsociety/EvoHime/issues/25). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **External Coding Agent Adapter**: Core-owned слой, позволяющий запускать и подключать внешние coding-agent процессы через versioned typed protocol вместо интеграции каждого CLI отдельным набором shell-скриптов.

Внешний агент управляет собственным внутренним LLM/tool loop, а EvoHime остаётся владельцем:

- lifecycle процесса;
- выбранного workspace;
- credentials/materialization policy;
- conversation/run identity;
- UI projection;
- approvals и локальных security boundaries, которые можно гарантировать снаружи;
- audit/provenance.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/external-coding-agent-adapter.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./45-1-external-coding-agent-adapter.md)
- [Этап 2 — runtime-интеграция и recovery](./45-2-external-coding-agent-adapter.md)
- [Этап 3 — IPC, client projection и UI](./45-3-external-coding-agent-adapter.md)
- [Этап 4 — verification, release-evidence и закрытие](./45-4-external-coding-agent-adapter.md)

## Зависимости

### Блокирующие

- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- external agent executable identity Core-owned/user-approved;
- command не собирается через shell string interpolation;
- credentials передаются только declared slots;
- environment deny-by-default;
- per-conversation data dir где возможно;
- process tree supervised;
- external agent не расширяет Core capability registry;
- control level отображается честно;
- opaque executor не получает ложный статус «fully approval controlled»;
- secrets/materialized files redacted из trace.

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

- [ ] Есть versioned external-agent protocol adapter.
- [ ] Есть Core-owned preset registry.
- [ ] Process lifecycle управляется supervisor.
- [ ] Есть capability handshake.
- [ ] Conversation фиксирует immutable agent snapshot.
- [ ] Credentials materialize только через declared slots.
- [ ] Параллельные conversations изолируют mutable agent state где возможно.
- [ ] UI различает уровень Core control над external executor.

## Ограничения и non-goals

- reverse engineering закрытых CLI без стабильного protocol;
- обещание полного per-tool контроля над opaque agents;
- автоматическая установка произвольных agent packages из интернета;
- перенос provider subscription credentials между машинами;
- запуск custom shell command из prompt;
- замена собственного EvoHime agent runtime.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#25 External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol](https://github.com/rkfsociety/EvoHime/issues/25)
