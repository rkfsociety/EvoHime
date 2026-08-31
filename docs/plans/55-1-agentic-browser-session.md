# План 55.1 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой: Core-контракт, schema и storage

Статус: этап 1 для [плана 55.0](./55-0-agentic-browser-session.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/35). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой» и сделать его реализуемым: первичный выход — «Есть Core-owned BrowserSession lifecycle».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/agentic_browser_session.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 55.0 — scope, requirements, non-goals и dependency map.
- Existing `browser.session.*`, CDP session registry and SSRF implementation
  as explicit migration baseline.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Execution Policy Profiles adapter из overview.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
   Define typed open/snapshot/click/fill/select/press/scroll/wait/history/
   download/upload/close commands, per-action risk class and policy snapshot;
   sensitive submit/upload/auth actions still pass the ordinary approval path.
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

- `agentic_browser_session.rs` определяет lifecycle/policy/ref contract, а
  `tool-runtime/src/tools/browser_session.rs`, `cdp.rs` и `ssrf.rs` мигрируют
  с raw CSS/env-CDP semantics. Parallel tool names/backend state запрещены.
- Зафиксировать packaged backend choice, ownership и launch/cleanup contract:
  isolated ephemeral profile обязателен, arbitrary user CDP endpoint не
  является production default, внешний Node/Python runtime не требуется.
- Session/page/element refs ephemeral; durable хранится только bounded
  lifecycle/audit metadata. Screenshots/downloads идут в ArtifactStore, не
  пишутся прямо в workspace.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/agentic_browser_session_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть Core-owned BrowserSession lifecycle. → зафиксировать typed invariant, error code и deterministic fixture.
- `C03` — Refs имеют page revision и stale protection. → зафиксировать fingerprint, preconditions и provenance-поля.
- `C04` — Есть network/SSRF policy с private-address protection. → зафиксировать typed invariant, error code и deterministic fixture.
- Redirect hops, post-resolution IP и DNS rebinding входят в тот же policy;
  initial-URL-only проверка не закрывает критерий.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть Core-owned BrowserSession lifecycle.
- [ ] Модель работает через typed browser tools и stable element refs.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
