# План 32.1 — Conversational Workflow Composer: создание и правка workflow из естественного языка: Core-контракт, schema и storage

Статус: этап 1 для [плана 32.0](./32-0-conversational-workflow-composer.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/12). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative proposal/draft contract «Conversational Workflow Composer: создание и правка workflow из естественного языка» и сделать его реализуемым: первичный выход — versioned separation между model proposal и Core-owned `workflow/v1` draft.

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/conversational_workflow_composer.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 32.0 — scope, requirements, non-goals и dependency map.
- План 31.0 — builder contract для последующего открытия draft.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 30.0 — зависимость из обзора.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input, malformed model response, catalog drift и authority-bearing unknown data дают typed error.
3. Зафиксировать lifecycle: proposal и unsaved draft session ephemeral, accepted graph сохраняется только через общий Builder authoring/storage contract как immutable `workflow/v1` revision. Отдельный Composer store не добавлять; positive save/reload и negative unsaved-restart tests должны доказать это решение.
4. Описать bounded model request/response: выбранный Core model route, catalog snapshot/hash, max input/output/repair loops, timeout/cancellation, model/version metadata и redacted provenance hashes; raw prompt/output и hidden reasoning не входят в durable contract.
5. Добавить deterministic fixtures: valid/invalid, unknown/ambiguous binding, duplicate, stale, catalog drift, malformed/oversized response, redaction, limit, save/reload и model-unavailable; выдать evidence-пакет этапу 2.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Предметная декомпозиция

### Поверхности и контракт

- `crates/evohime-core/src/conversational_workflow_composer.rs`: ввести `ConversationalWorkflowComposerDefinition`, `ConversationalWorkflowComposerPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: proposal/unsaved draft остаются ephemeral; accepted definition/revision и provenance сохраняются через общий Builder authoring/storage contract. Новую Composer-specific durable таблицу и migration не добавлять. Нужны negative persistence test для unsaved session и positive immutable save/reload test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/conversational_workflow_composer_contract.rs` для valid/invalid, bounds, catalog/model response, redaction, duplicate/stale, immutable save/reload и ephemeral unsaved решения.

### Acceptance-to-contract matrix

- `C03` — Core выполняет capability binding и validation. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C05` — Missing integrations/credentials показываются отдельно. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C08` — Composer не может расширить permissions или выполнить draft самовольно. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C01` — Ева умеет создавать workflow draft из natural language. → bounded model request/response, catalog snapshot и typed unavailable/invalid outcomes.
- `C02` — Proposal отделён от authoritative workflow contract. → отдельные proposal/draft types и запрет прямой десериализации proposal в runnable graph.
- `C04` — Есть iterative typed edits. → versioned operation enum с stable node IDs и optimistic draft revision.
- `C06` — Есть risk/side-effect preview. → typed projection contract без raw model output и без approval authority.
- `C07` — Draft можно открыть в builder и сохранить как immutable version. → reuse Builder authoring/storage contract, а не новый Composer store.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Conversational Workflow Composer: создание и правка workflow из естественного языка».
- Evidence stage 1: `cargo test --locked -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые contract fixtures; SQL migration evidence требуется только если Builder authoring contract меняет schema.

## Критерии выхода

- [ ] Есть versioned proposal/draft contract и bounded model/catalog boundary.
- [ ] Proposal отделён от authoritative workflow contract.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.
- [ ] Model/catalog bounds, provenance redaction и typed unavailable/invalid outcomes доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
