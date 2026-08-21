# План 16.1. Definition, trigger и run contract

## Цель

Описать versioned модель автоматизации, которая одинаково работает для ручного,
расписанного, повторного и simulation запуска.

## Изменения

- Ввести `AutomationDefinition` с stable id/revision, bounded steps,
  capability references, schedule/trigger policy, owner scope и retention.
- Разделить trigger request, accepted run, activity event, health snapshot и
  terminal outcome; UI получает read-only projection.
- Сделать обязательными idempotency key, correlation id, permission snapshot,
  approval policy snapshot и generation при создании run.
- Описать cancellation, pause/resume, retry и typed provider/supervisor errors;
  повторная доставка события должна быть безопасной.
- Ограничить число шагов, payload, history и parallelism; arbitrary child-agent
  graph не является частью definition.

## Проверки

- schema/version/size validation и отказ неизвестных опасных полей;
- duplicate trigger с тем же idempotency key;
- одинаковая definition с разными revision и scope;
- approval/permission snapshot фиксируется для run и не подменяется UI;
- projection не содержит секретов или неограниченного provider output.

## Готово, когда

Каждый run имеет stable identity, bounded definition, immutable launch
snapshots и typed lifecycle, а trigger можно безопасно повторить после timeout
или reconnect.

