# 08-3 — IPC replay и Core projection

## Цель

Передать typed execution history через authenticated desktop IPC так, чтобы
Electron мог восстановить projection после reconnect, но не мог изменить
durable ledger или расширить permissions.

## Изменения

1. Добавить bounded protobuf `ExecutionEvent` projection с common correlation
   fields и typed oneof body внутрь существующего `EventEnvelope`; generic
   `event_type/payload` оставить для backward compatibility и legacy mapping.
2. Использовать существующие `core_instance_id`/`session_epoch` как generation
   identity, а `sequence_id` как ledger cursor. `ReplayGap` должен сообщать
   requested-after, earliest и latest sequence; mismatch generation обозначать
   отдельным bounded stale reason. Не вводить расплывчатый "Core revision",
   пока для него нет отдельного monotonic contract.
3. Сохранить порядок глобальной `sequence_id`, ограничение frame/payload size
   и duplicate suppression в Electron main по `(core_instance_id,
   session_epoch, event_id)`; для legacy событий использовать
   `(generation, sequence_id)`. Повторная команда дополнительно дедуплицируется
   Core по action/approval/idempotency key, а не только UI-состоянием.
4. В Electron main adapter преобразовывать IPC event в bounded redacted
   projection; renderer получает только чтение и пользовательские approval
   decisions через штатный Core command path. `ExecutionEvent` не принимается
   как inbound mutation, а каждое approval решение повторно проходит Core
   exact-call/policy/terminal checks.
5. При смене Core instance/session очищать stale projection и запрашивать
   bounded replay. `FullSnapshot` должен быть настоящим versioned snapshot
   action projection с `snapshot_sequence_id`, generation и terminal states,
   а не копией произвольного хвоста `events`; размер snapshot и число action
   rows ограничиваются теми же лимитами, что и replay.

## Проверки

- protocol generation и major-version compatibility;
- replay после reconnect во время running, approval и cancellation;
- gap/stale generation и full snapshot fallback с проверкой snapshot cursor;
- отсутствие дублей и сохранение исходного порядка;
- старый клиент, который игнорирует additive `ExecutionEvent`, не ломает
  generic event replay;
- negative tests на попытку изменить action/tool/scope через IPC;
- Electron typecheck, adapter tests и real-Core E2E.

## Готово, когда

После reconnect пользователь видит ту же action card и terminal state, cursor
не перескакивает между Core generations, gap не маскируется пустым ответом, а
повтор доставки UI-команды не создаёт новый effect и не меняет журнал
напрямую.
