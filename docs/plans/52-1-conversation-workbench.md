# План 52.1 — Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage: Core-контракт, schema и storage

Статус: этап 1 для [плана 52.0](./52-0-conversation-workbench.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/32). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage» и сделать его реализуемым: первичный выход — «Есть единый Conversation Workbench рядом с chat».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/conversation_workbench.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 52.0 — scope, requirements, non-goals и dependency map.
- Conversation Event Log v1 и TaskCheckpoint v1 projection contracts.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Agentic Browser Session и Revision-Safe Workspace Files capabilities из overview.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
   Ввести built-in `WorkbenchTabDescriptor` registry с required capabilities,
   availability reason, persistence policy и badge source; workflow/skill не
   может зарегистрировать executable tab.
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

- Core projection composer использует `conversation_event_log.rs`,
  TaskCheckpoint и existing tool/event refs; новый workbench runtime или
  task/event store не создаётся.
- Core projection ephemeral. Только content-free presentation state (active
  tab, split sizes, collapsed groups) хранится shell-side per conversation с
  bounded schema/version; raw terminal/browser/file content не persists.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/conversation_workbench_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C03` — Все authoritative операции проходят Core services. → зафиксировать typed invariant, error code и deterministic fixture.
- `C08` — Sensitive data и unavailable capabilities корректно ограничены. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- Availability вычисляется по capability handshake/snapshot, а не по имени
  backend; Files/Diff/Tasks/Terminal/Browser/Usage имеют отдельные typed source
  contracts и честный `unavailable`.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть единый Conversation Workbench рядом с chat.
- [ ] Files/Diff/Tasks/Terminal/Browser/Usage представлены отдельными capability-aware tabs.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
