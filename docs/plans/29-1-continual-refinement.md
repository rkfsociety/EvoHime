# План 29.1 — Continual Refinement с evidence и approval: Core-контракт, schema и storage

Статус: этап 1 для [плана 29.0](./29-0-continual-refinement.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/4). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Continual Refinement с evidence и approval» и сделать его реализуемым: первичный выход — «repeated evidence создаёт candidate, единичная ошибка не создаёт global rule».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: crates/evohime-core, crates/evohime-local-storage, crates/desktop-ipc, Electron и focused tests. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

Предварительные точки интеграции (не являются заранее утверждённым API):
`crates/evohime-core/src/refinement.rs`, отдельный storage store в
`crates/evohime-local-storage`, существующие memory-domain и
`skill_registry`/capability policy surfaces. Для PromptRule stage 1 обязан
либо определить Core-owned registry contract, либо зафиксировать typed
`unavailable`; запись markdown-файла или изменение capability registry моделью
не допускается.

## Зависимости

### Блокирующие

- План 29.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input и authority-bearing unknown data дают typed error.
3. Добавить durable store и additive migration с backup-before-migrate только если состояние переживает restart; ephemeral state закрепить отрицательным persistence test.
4. Добавить deterministic fixtures: valid/invalid, duplicate, stale, redaction, limit и migration failure; выдать evidence-пакет этапу 2.

### Предметная декомпозиция

- Contract: определить `RefinementCandidateV1`, `EvidenceRefV1`,
  `EvaluationResultV1`, `ActivationRecordV1` и typed errors. Обязательны
  owner scope, target/kind, immutable revision, `pattern_key`, content hash,
  source task ids, independent-evidence count, policy snapshot hash,
  idempotency key и optimistic version.
- Admission policy: зафиксировать таблицу порогов для kind × scope, правила
  независимости task/source, time window, size/count/retention limits,
  sensitive-content handling и typed reasons `insufficient_evidence`,
  `duplicate`, `conflict`, `source_unavailable`, `policy_denied`.
- Storage: хранить candidate revisions, evidence links, evaluation attempts,
  activation/rollback records и append-only events в additive migration с
  backup-before-migrate. Raw transcript и unrestricted candidate content в
  event journal не попадают; ephemeral reflection input получает negative
  persistence test.
- Tests: добавить fixtures для одной ошибки, repeated independent evidence,
  same-task duplicates, conflicting/deleted source, all scopes/kinds,
  redaction, retention, hash/idempotency, migration rollback и unknown target.

### Acceptance-to-contract matrix

- `R29-C01` → `pattern_key`, independent source/task counting и admission fixtures.
- `R29-C02` → versioned types, immutable revisions, bounds, canonical hash и
  storage schema.
- `R29-C03` → duplicate/conflict/source invalidation/evaluation typed errors.
- `R29-C04`/`R29-C05` → target adapter contract, policy snapshot и
  unavailable/approval states без capability expansion.
- `R29-C06` → activation/rollback/provenance tables и idempotent transitions.
- `R29-C08` → redaction, retention, migration and adversarial contract tests.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Критерии выхода

- [ ] repeated evidence создаёт candidate, единичная ошибка не создаёт global rule.
- [ ] candidate имеет scope, provenance, content hash и durable lifecycle.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.
- [ ] Порог evidence, независимость источников, bounds/retention и PromptRule
  owner или typed `unavailable` зафиксированы deterministic fixtures.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
