# План 16.3. Snapshots, recovery и simulation

## Цель

Добавить воспроизводимое восстановление долгих runs и simulation harness без
доступа к production side effects.

## Изменения

- Разделить durable snapshot, derived diff, activity history и live projection;
  snapshot должен быть bounded, versioned и связан с definition/run generation.
- После crash восстанавливать только валидный snapshot, проверять lease,
  generation, policy snapshot и provenance, затем продолжать либо typed-block.
- Ввести deterministic simulation clock, fake provider boundary, fixed input
  fixtures и replay hash; simulation явно помечается и не может использовать
  production credentials или external effects.
- Архивировать завершённые runs отдельно от active state с bounded retention и
  возможностью export redacted history.
- Определить rollback/repair для битого snapshot, неполного diff и устаревшей
  definition revision.

## Проверки

- snapshot/diff recovery после crash на каждом состоянии;
- deterministic replay при одинаковом clock/input/provider fixture;
- simulation попытки вызвать filesystem/network/host action;
- corrupted/oversized snapshot, stale generation и incompatible schema;
- archive/retention consistency и redaction при export.

## Нормативные контракты реализации

- **Snapshot и инварианты.** Snapshot — атомарная запись до 1 MiB, schema
  version `1`, содержащая `run_id`, `definition_id`, `definition_revision`,
  `fencing_generation`, last durable event sequence, policy/approval snapshot
  и provenance. Snapshot valid только при корректной checksum, непрерывной
  diff chain от предыдущего sequence и совпадении definition revision;
  authoritative state/history остаются в SQLite, а projection всегда
  пересобирается из них. Generation только монотонно возрастает.
- **Recovery state machine.** После crash Core в фиксированном порядке проверяет
  checksum/size → schema compatibility → diff continuity → definition
  revision → policy/approval/provenance → lease/generation. Успех переводит run
  в `queued` с recovery event; несовместимая schema допускает только
  allow-listed migration, иначе `typed-block` с кодом
  `incompatible_schema`; повреждение — `corrupt_snapshot`, разрыв цепи —
  `incomplete_diff`, устаревшая revision — `stale_definition`, policy или
  provenance mismatch — `invalid_provenance`. `typed-block` terminal до
  explicit repair/retry команды и никогда не запускает effect автоматически.
- **Replay contract.** Replay hash — SHA-256 канонического CBOR/JSON набора
  `(schema_version, definition_revision, ordered durable events, normalized
  inputs, frozen clock, RNG seed, provider fixture IDs, capability/policy
  snapshot)`, без локальных путей, секретов и timestamps вне frozen clock.
  Одинаковый набор обязан дать одинаковый hash; mismatch — typed
  `replay_mismatch`, effect не выполняется.
- **Simulation enforcement.** Simulation запускается в отдельной ephemeral
  SQLite/temp workspace с отдельным capability token и пустыми credentials;
  fake provider boundary — единственный provider adapter. Runtime guards
  fail-closed на filesystem вне temp root, network, process/shell, registry,
  clipboard и production IPC; попытка получает `simulation_external_effect`
  и audit event. Тесты проверяют каждый guard, а не только флаг режима.
- **Archive, repair и retention.** Перенос active → archive, history export и
  redaction выполняются транзакционно; archive содержит ту же checksum и
  provenance, redaction удаляет secrets/credentials/absolute paths, сохраняя
  event type, sequence и причины. Retention: active snapshot chain ≤64
  snapshots/run, archive ≤10 000 runs и 30 дней, export ≤10 MiB. Automatic
  repair разрешён только для восстановления projection/diff из валидного
  snapshot; rollback definition или ручная правка history требуют explicit
  command, audit и backup.
- **Наблюдаемость и SLO.** Пишутся audit events и metrics для recovery result,
  typed-block reason, replay mismatch, simulation violation, archive/repair и
  snapshot size. Recovery p95 ≤2 s для snapshot до 1 MiB, snapshot overhead
  ≤10% от durable event write time; превышение лимитов даёт typed failure, а
  не silent truncation.

## Готово, когда

Run воспроизводимо восстанавливается или получает стабильный typed-block с
причиной; snapshot/diff/history/projection проходят инварианты и version
compatibility checks, simulation guards fail-closed, archive/active операции
транзакционны, retention/redaction bounded, а recovery/replay/repair покрыты
измеримыми тестами и метриками.
