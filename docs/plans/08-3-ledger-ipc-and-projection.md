# 08-3 — IPC replay и Core projection

## Цель

Передать typed execution history через authenticated desktop IPC так, чтобы
Electron мог восстановить projection после reconnect, но не мог изменить
durable ledger или расширить permissions.

## Зависимости

### Блокирующие

- [08-2](08-2-ledger-storage-and-recovery.md): durable typed events, атомарные
  переходы и recovery-классификация;
- текущий authenticated desktop IPC: `EventEnvelope` с `sequence_id`,
  `core_instance_id`, `session_epoch`, `ReplayGap`, `FullSnapshot`, frame limit
  и генерация протокола (`npm run check:protocol`);
- Electron main adapter и renderer isolation в `desktop/evohime-electron`.

### Опциональные

- Action Console из 07-3: при её наличии projection питает готовый UI, иначе
  проверяется adapter-тестами и существующими экранами без нового UI;
- `CoreInfo`/negotiation из 10-1: до него generation identity остаётся парой
  `core_instance_id`/`session_epoch`, и никакой отдельный «Core revision» не
  вводится.

## Что уже есть в коде

- `EventEnvelope` уже несёт generic `event_type`/`payload`, generation identity
  и oneof с `ready`/`replay_gap`/`full_snapshot`/`auth_challenge` (номера
  10–13);
- `ReplayGap` уже содержит `requested_after_sequence`,
  `earliest_available_sequence`, `latest_available_sequence` и `reason`;
- `FullSnapshot` уже содержит `sequence_id` и `snapshot_json`, ограниченный
  frame limit.

Нет typed `ExecutionEvent` в envelope, нет versioned action-проекции внутри
snapshot и нет дедупликации доставки по `event_id`.

## Изменения

1. Добавить bounded protobuf `ExecutionEvent` projection с common correlation
   fields и typed oneof body внутрь существующего `EventEnvelope` новым
   свободным номером oneof (10–13 заняты); generic `event_type/payload`
   оставить для backward compatibility и legacy mapping.
2. Использовать существующие `core_instance_id`/`session_epoch` как generation
   identity, а `sequence_id` как ledger cursor. Поля `ReplayGap` уже
   достаточны: их надо заполнять честно и добавить bounded набор допустимых
   значений `reason`, включая отдельный stale-generation reason. Не вводить
   расплывчатый «Core revision», пока для него нет отдельного monotonic
   contract.
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
   bounded replay. Содержимое `FullSnapshot` должно стать versioned snapshot
   action projection с `snapshot_sequence_id`, generation и terminal states, а
   не копией произвольного хвоста `events`; само поле snapshot остаётся
   additive-совместимым с текущим сообщением, а размер snapshot и число action
   rows ограничиваются теми же лимитами, что и replay.

## Проверки

- protocol generation и major-version compatibility (`npm run check:protocol`);
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
