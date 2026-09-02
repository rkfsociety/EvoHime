# План 116.1 — Local Model Runtime Manager: Core-контракт, schema и storage

Статус: этап 1 для [плана 116.0](./116-0-local-model-runtime-manager.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/96). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать versioned contract, trust/provenance matrix, limits, state
transitions, persistence decision и canonical serialization для Local Model Runtime
Manager. Первичный выход: Core умеет доказуемо описать hardware, exact model/runtime
revision и fit, не запуская внешний процесс.

## Зависимости

### Блокирующие

- План 116.0 — scope, MVP backend, acceptance и security boundary.
- Model Gateway/ModelProfile, Model Resilience Policy, Execution Backend Registry,
  Context Budget, SQLite backup-before-migrate и event journal.

### Опциональные

- План 36, 41, 46, 53 и 105 — как указано в overview; отсутствие даёт описанную
  conservative/degraded behavior.

## Реализация

0. Сверить overview с live code, schema v61, existing model/backend stores и
   свободными IPC tags; имена поверхностей не считать API до evidence freeze.
1. Ввести Core types/validators для hardware, accelerator/storage, catalog
   descriptor, runtime identity, fit, context profile, lifecycle, failure reasons,
   activation и resource policy. Ограничить строки, массивы, bytes, paths и counts.
2. Определить privacy-safe fingerprint (локальный cache key, без serial/MAC/PII),
   deterministic canonical JSON/hash и sensitivity/provenance table. Unknown enum,
   schema, trust, format, runtime identity и oversized input дают typed fail-closed.
3. Добавить additive durable store и migration: catalog revisions, artifact
   records, runtime registry, preferred/bootstrap policy и session metadata. Не
   хранить prompt/output/credentials; migration transactional, backup-before-migrate,
   rollback и corruption tests обязательны. Download progress и process handles
   остаются ephemeral либо recovery metadata-only.
4. Определить exact MVP runtime descriptor и модель trust levels
   `ManagedVerified/UserImported/Unverified`; imported/unverified не получают
   implicit execution authority. Зафиксировать, что runtime executable hash и
   artifact hash независимы.

## Предметная декомпозиция

- Кандидатные модули: `crates/evohime-core/src/local_model_runtime_manager.rs`,
  отдельный `hardware_profiler.rs` и storage store; точные имена подтвердить на
  реализации.
- Catalog revision immutable; новая model revision создаёт новый descriptor и
  не мутирует старый stable identity.
- `LocalModelFit` содержит status, estimated memory/headroom, recommended context,
  performance class, bounded reasons/warnings и input hashes.
- Path contract разрешает только Core-managed artifact root и rejects traversal,
  reparse/symlink escape и workspace-dependent location.

## Acceptance-to-contract matrix

- `C01` hardware profile versioned/Core-owned → typed snapshot, privacy-safe hash,
  invalidation key и refresh semantics.
- `C02` exact catalog/runtime identity → immutable revision/hash/trust fields.
- `C03` conservative fit → memory/context formulas, `Unknown` and bounded reasons.
- `C04` lifecycle safe → typed states, legal transitions, atomic promotion preconditions.
- `C05` Model Gateway compatibility → stable mapping to existing ModelProfile refs,
  capability/locality/context fields без второго profile authority.
- `C06` bootstrap/activation → policy enum, strict snapshot and call-boundary rules.

## Критерии выхода

- [ ] Contract, transitions, errors, bounds and canonical hash are tested.
- [ ] Additive schema revision is selected from live `SCHEMA_VERSION`, with backup/rollback.
- [ ] No secrets, raw conversation content or executable authority enters durable records.
- [ ] Negative tests prove unknown/unverified/staging/traversal are fail-closed.

## Не входит

Process launch, network download, hardware OS probing implementation, IPC/UI и
Model Gateway dispatch.

