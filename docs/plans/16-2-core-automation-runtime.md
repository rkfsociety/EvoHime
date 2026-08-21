# План 16.2. Core queue, state machine и leases

## Цель

Реализовать single-owner execution model для automation runs с защитой от
overlap, stale generation и неконтролируемого давления очереди.

## Изменения

- Владелец queue/state machine — Core; durable run state и history сохраняются
  через SQLite/ledger contracts, renderer не принимает execution decisions.
- Разделить durable step transitions от высокочастотных tick/progress messages;
  применить bounded queue, backpressure и coalescing только там, где это
  разрешено контрактом.
- Добавить generation/monotonic lease и operation lock для async provider calls;
  старый runner не может подтвердить шаг после takeover или restart.
- Сделать cancellation кооперативной с deadline и typed outcome; supervisor
  restart переводит run в recoverable state, а не теряет audit.
- На каждом фактическом effect повторно проверить scope, capability, policy и
  approval snapshot, даже если trigger был ранее принят.

## Проверки

- duplicate/overlapping runs, stale generation и lease takeover;
- tick ordering, queue backpressure и bounded retry;
- provider timeout/error, cancellation, Core crash и supervisor restart;
- запрет side effect без актуальной policy/approval revalidation;
- отсутствие scheduler/executor logic в renderer.

## Готово, когда

Только текущий Core owner меняет run state, старые поколения безопасно
отбрасываются, очередь и provider calls ограничены, а восстановление оставляет
проверяемые typed history и terminal outcome.

