# План 34.1 — Event Trigger Runtime: безопасный запуск workflow по внешним событиям: Core-контракт, schema и storage

Статус: этап 1 для [плана 34.0](./34-0-event-trigger-runtime.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/14). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Event Trigger Runtime: безопасный запуск workflow по внешним событиям» и сделать его реализуемым: первичный выход — «Есть versioned TriggerDefinition».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/event_trigger_runtime.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 34.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 33.0 — provider trigger capability declarations and provider identity
  contract; без него provider-originated events остаются typed `unavailable`,
  а local/system source kinds реализуются без provider SDK.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input и authority-bearing unknown data дают typed error.
3. Добавить durable store и additive migration с backup-before-migrate только если состояние переживает restart; ephemeral state закрепить отрицательным persistence test.
4. Добавить deterministic fixtures: valid/invalid, duplicate, stale, redaction, limit и migration failure; выдать evidence-пакет этапу 2.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Предметная декомпозиция

### Поверхности и контракт

- `crates/evohime-core/src/event_trigger_runtime.rs`: ввести `EventTriggerRuntimeDefinition`, `EventTriggerRuntimePolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/event_trigger_runtime_store.rs` и существующий `LocalDatabase` migration path; durable schema должна охватывать definition/version, subscription status, accepted pending events, bounded dedup journal, last execution ref, rate/circuit state и reconnect/error state. Migration additive, backup-before-migrate, rollback без частичной записи.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/event_trigger_runtime_contract.rs` для всех source kinds, valid/invalid authenticity/schema, bounds, mapping, redaction, duplicate/stale, subscription states, circuit outcomes и migration/restart решения.

### Acceptance-to-contract matrix

- `C01` — Есть versioned TriggerDefinition. → определить immutable version, content hash, stable workflow binding и typed lifecycle.
- `C02` — Есть normalized EventEnvelope. → определить source/event/schema/received timestamps, payload ref-or-bounded-inline, hash, sensitivity, authenticity и correlation.
- `C03` — Webhook authenticity и schema проверяются до enqueue. → определить provider validation strategy, credential ref, content-type/size limits и fail-closed errors.
- `C04` — Есть dedup/replay protection. → определить stable provider event key и bounded payload-hash/time-window fallback, TTL и `duplicate_ignored` outcome.
- `C05` — Workflow version pinned. → запретить binding к «последней версии» и зафиксировать immutable workflow version/hash.
- `C06` — Input mapping ограничивает payload. → зафиксировать allowlisted source paths, required fields, safe transforms и deterministic fixture.
- `C07` — Есть rate limits/circuit breaker. → определить per-trigger event/minute, concurrency, queue depth, coalescing/overflow и typed circuit states.
- `C08` — State durable/recoverable. → определить additive tables, accepted marker, pending event recovery и backup/rollback evidence.
- `C09` — Existing workflow approvals/grants сохраняются. → зафиксировать provenance/policy references и invariant, что trigger не является approval или capability grant.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Event Trigger Runtime: безопасный запуск workflow по внешним событиям».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned TriggerDefinition.
- [ ] Есть normalized EventEnvelope.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
