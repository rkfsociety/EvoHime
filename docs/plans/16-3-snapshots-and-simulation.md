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

## Готово, когда

Run воспроизводимо восстанавливается или получает объяснимый typed-block,
simulation полностью изолирована, active/history state не смешиваются, а
replay подтверждается фиксированным hash.

