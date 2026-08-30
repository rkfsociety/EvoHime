# План 35.1 — Invocation Presets: version-pinned шаблоны запусков без копирования секретов: Core-контракт, schema и storage

Статус: этап 1 для [плана 35.0](./35-0-invocation-presets.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/15). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Invocation Presets: version-pinned шаблоны запусков без копирования секретов» и сделать его реализуемым: первичный выход — «Есть durable InvocationPreset contract».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/invocation_presets.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 35.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 33.0 — зависимость из обзора.
- План 30.0 — только optional portable export/import; локальный preset должен
  работать без package-контуров.
- Event Trigger Runtime v1 — только optional trigger base mapping; без него preset остаётся
  usable для manual/schedule запуска.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input и authority-bearing unknown data дают typed error.
3. Добавить durable store и additive migration с backup-before-migrate только если состояние переживает restart; ephemeral state закрепить отрицательным persistence test.
4. Добавить deterministic fixtures: valid/invalid, duplicate, stale, redaction, limit и migration failure; выдать evidence-пакет этапу 2.

### Обязательная точность контракта

- `input_values` и `execution_options` задаются явной allowlist-таблицей типов
  и полей. Запрещаются graph/node/provider/action identity, capability/grant,
  approval policy, executable/path/network routing, raw credential и secret
  input; неизвестное authority-bearing поле даёт typed rejection, а не
  игнорируется.
- Completed-run sanitizer принимает только Core-owned metadata из успешного
  завершённого workflow run. Он возвращает bounded preview с retained,
  removed и rejected field names; неизвестные или неоднозначные поля не
  сохраняются молча. Tokens, ephemeral IDs, absolute paths, artifact bodies,
  trigger payload, prompt/output/transcript и credentials должны иметь
  отрицательные fixtures.
- Migration contract хранит source/target workflow definition и input-schema
  hashes, mapping revision и explicit actor/provenance. Mapping может быть
  только Core-валидированным allowlist-описанием; до commit есть preview,
  а несовместимость даёт `NeedsMigration` или `IncompatibleSchema` без записи.
- Для scheduler stage 1 должен определить typed reference/snapshot fields,
  которые будут добавлены в `automation_schedules`/run admission, чтобы
  `preset_id`, revision и content hash не потерялись между automation и
  workflow runtime.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- immutable revision, workflow/schema snapshot hashes, run sanitizer and
  schedule snapshot contract;
- focused contract/security/migration tests.

## Предметная декомпозиция

### Поверхности и контракт

- `crates/evohime-core/src/invocation_presets.rs`: ввести
  `InvocationPresetDefinition`, `InvocationPresetPolicy`, typed state/event/error
  types и public validation entrypoint; зарегистрировать модуль в
  `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/invocation_presets_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/invocation_presets_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale, owner scope, immutable revision, schema hashes, completed-run sanitization, credential refs/NeedsRebinding и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C04` — Credentials хранятся только как refs. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C05` — Secret inputs не сохраняются raw по умолчанию. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C01` — Есть durable InvocationPreset contract. → определить owner scope, immutable revision, canonical redacted hash и durable schema.
- `C02` — Preset pinned к workflow version. → хранить workflow definition hash и input schema hash; mismatch становится typed drift.
- `C03` — Можно создать preset из completed run. → определить allowlist invocation metadata и fail-closed sanitizer для ephemeral IDs, tokens, paths, artifacts и trigger payload.
- `C06` — Есть migration flow между workflow versions. → определить compatible mapping, required user mapping и typed incompatible outcomes; silent migration запрещена.
- `C09` — Preset можно создать вручную из workflow detail. → валидировать inputs against frozen schema до persistence.
- `C10` — Удалённый/expired credential даёт `NeedsRebinding`. → определить binding status и отсутствие secret cache.
- `C11` — Временный override не изменяет сохранённую revision. → разделить persisted payload и run-only overlay.
- `C12` — Schedule фиксирует revision/hash snapshot. → определить typed schedule reference/snapshot без зависимости от package import.
- `C13` — Version drift показывает preview и не выполняет silent migration. → определить hash comparison и migration preconditions.
- `C14` — Trigger base mapping optional и не переопределяет protected identities. → определить bounded mapping contract и fail-closed unavailable fallback.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Invocation Presets: version-pinned шаблоны запусков без копирования секретов».
- Зафиксировать canonical serialization (порядок ключей, omission/defaults,
  Unicode/number normalization и SHA-256 input) одной общей fixture для
  storage, runtime, schedule и IPC; hash не включает secret material.
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть durable InvocationPreset contract.
- [ ] Preset pinned к workflow version.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
