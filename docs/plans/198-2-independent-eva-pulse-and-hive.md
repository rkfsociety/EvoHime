# План 198.2 — Runtime isolation, delivery и recovery

## Текущий прогресс

Добавлен отдельный `EvaPulseController`: IPC получает только narrow lifecycle
control/status owner, а не полный daemon с probe/task sink. Остановка Pulse
через этот owner не обращается к Hive task state и не отменяет уже принятые
Hive-задачи. Полное разделение bootstrap, независимый recovery и typed signal
receiver остаются в scope этапа.

Добавлен versioned `EvaPulseSnapshot` с явным owner `eva_pulse`; он может быть
передан отдельным IPC projection без включения Pulse lifecycle в Hive-owned
snapshot. Текущий legacy `hive_state.snapshot` и `togglePulse` пока сохранены
для обратной совместимости. Добавлена additive-команда `EvaPulseStatusCommand`
и response kind `eva_pulse.status`; старый `togglePulse` остаётся compatibility
alias только для управления включением daemon.

## Контракт runtime

- EvaPulse владеет только собственным scheduler, status, cancellation,
  outbox и retry/dead-letter.
- Hive boundary владеет приёмом сообщения, решением Королевы и созданием
  Hive-owned task через обычный coordinator path.
- Принятые Hive tasks переживают остановку EvaPulse; непринятые сообщения не
  создают task и остаются в EvaPulse outbox/dead-letter.

## Работа

1. Разделить bootstrap и lifecycle owners.
2. Реализовать bounded adapter `EvaPulseClient`/`HiveSignalReceiver`.
3. Передавать только typed request в Королеву; её решение и task binding
   фиксировать в журнале.
4. Добавить fencing по correlation/message id и deterministic retry policy.
5. Убрать `togglePulse` как способ управления внутренним состоянием Улья;
   отдельные команды управляют соответствующей системой.

## Verification

- независимый start/stop/crash/restart каждого runtime;
- принятие, отклонение, defer, duplicate и TTL expiry;
- EvaPulse crash не отменяет Hive task;
- Hive restart не запускает EvaPulse cycle автоматически;
- supervisor/heartbeat и bounded-resource tests.
