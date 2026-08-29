# План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake

Статус: предложено по [issue #23](https://github.com/rkfsociety/EvoHime/issues/23). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Execution Backend Registry**: единый реестр локальных и удалённых backend-окружений, на которых могут выполняться agent conversations/workflows, с явной проверкой совместимости, health, authentication references и capability handshake.

Backend здесь — не модель и не tool. Это конкретное runtime-окружение, которое предоставляет агентный execution API.

Примеры:

- локальный EvoHime Core/runtime;
- выделенный agent host на другой машине;
- sandbox/container host;
- VM/сервер;
- будущий совместимый remote executor.

Локальный backend остаётся default. Удалённые окружения являются optional extension.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/execution_backend_registry.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./43-1-execution-backend-registry.md)
- [Этап 2 — runtime-интеграция и recovery](./43-2-execution-backend-registry.md)
- [Этап 3 — IPC, client projection и UI](./43-3-execution-backend-registry.md)
- [Этап 4 — verification, release-evidence и закрытие](./43-4-execution-backend-registry.md)

## Зависимости

### Блокирующие

- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- endpoint canonicalized и validated;
- auth secrets остаются внутри Core credential boundary;
- renderer получает masked auth state, не секрет;
- backend handshake не может расширить пользовательские grants;
- advertised capability всё равно проходит local Core policy;
- unknown/unsupported backend не запускает agent process;
- TLS/auth requirements для remote backend configurable policy;
- backend-provided URLs не становятся executable capabilities автоматически;
- смена endpoint/auth инвалидирует caches через `connection_revision`.

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

- [ ] Есть durable Core-owned backend registry.
- [ ] Есть versioned capability/compatibility handshake.
- [ ] Есть health model с typed failures.
- [ ] Conversation/run фиксирует backend snapshot.
- [ ] Default backend отделён от active-run affinity.
- [ ] Advertised capabilities проходят Core policy.
- [ ] Credentials хранятся только как refs.
- [ ] Нет unsafe automatic failover side-effecting runs.

## Ограничения и non-goals

- обязательный cloud/SaaS backend;
- multi-tenant orchestration;
- автоматическая миграция running conversation между машинами;
- балансировка нагрузки между backend без явной semantics;
- arbitrary backend plugins из интернета;
- перенос security authority из EvoHime Core на remote backend.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#23 Execution Backend Registry: несколько agent backends, health и capability handshake](https://github.com/rkfsociety/EvoHime/issues/23)
