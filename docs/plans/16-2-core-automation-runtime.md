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

## Нормативные контракты реализации

- **FSM и durable source of truth.** Core хранит authoritative `RunRecord` и
  append-only `RunEvent` в одной SQLite-транзакции. Допустимые состояния run:
  `admitted`, `queued`, `starting`, `running`, `waiting_approval`, `retrying`,
  `cancelling`, `completed`, `failed`, `cancelled`, `dead_letter`; terminal —
  `completed`, `failed`, `cancelled`, `dead_letter`, recoverable —
  `queued`, `starting`, `running`, `waiting_approval`, `retrying`,
  `cancelling`. Переходы разрешены только через Core transition function и
  отклоняются typed `invalid_transition`; после terminal состояния переходы
  невозможны.
- **Lease/fencing.** На каждый run действует один lease с TTL 30 секунд,
  renewal не реже 10 секунд и монотонным `fencing_generation`. Acquire,
  renew, expire и takeover атомарно проверяют owner/generation; commit любого
  шага обязан предъявить актуальную пару `(run_id, fencing_generation)`, иначе
  получает `stale_generation` и не меняет state/effect. Takeover после expiry
  сначала пишет событие recovery, затем получает новый generation; старый
  runner не может продлить lease или завершить шаг.
- **Очередь и порядок.** Durable commands для одного run упорядочены по
  sequence; лимит очереди — 256 pending commands и 1024 progress/tick
  messages на Core. При переполнении command queue новый trigger получает
  typed `queue_full` без запуска, а progress coalescing оставляет только
  последнее значение на `(run_id, activity_id)` и никогда не удаляет durable
  transition. Никакого silent drop для command/event; порядок durable events
  задаётся SQLite sequence.
- **Provider-call contract.** Каждый provider call имеет deadline 120 секунд,
  cancellation token и operation id; после deadline Core отменяет запрос и
  записывает `provider_timeout`. Повторяются не более двух раз только
  allow-listed transient timeout/transport/provider-unavailable ошибок с
  bounded exponential backoff 1/2 секунды; policy, approval, validation,
  unsupported и schema errors не retry’ятся. Late success после cancellation,
  timeout или stale generation не коммитится и фиксируется как
  `late_result_ignored`; повторяемый effect требует тот же operation id.
- **Revalidation и recovery.** Непосредственно перед effect Core в одной
  preflight-проверке сверяет owner scope, capability set, policy revision и
  approval snapshot; mismatch даёт `policy_revalidation_failed` и effect не
  вызывается. После crash/restart незавершённый `running`/`starting`/`retrying`
  run переводится в `queued` с recovery event либо в `dead_letter`, если
  durable corruption обнаружена; provider call с неизвестным результатом не
  повторяется автоматически без idempotent operation id.
- **Граница renderer и типизированные результаты.** Renderer может только
  отправлять команды и читать projections; scheduler, lease, retry,
  preflight и state commit выполняются исключительно в Core. Обязательные
  typed outcomes/history events включают `started`, `transitioned`,
  `lease_acquired`, `lease_expired`, `takeover`, `stale_generation`,
  `provider_timeout`, `late_result_ignored`, `cancel_requested`,
  `cancelled`, `effect_rejected`, `recovered`, `completed` и `failed`.

## Готово, когда

Только текущий Core owner с актуальным fencing generation меняет run state;
FSM, lease protocol, bounded queue, provider-call policy и recovery проверяются
тестами, включая takeover, stale drop, queue pressure, cancellation и crash;
каждый фактический effect проходит scope/capability/policy/approval
revalidation, renderer не содержит execution decisions, а metrics/logs
показывают queue depth, lease conflicts, stale drops, cancel latency и recovery
time.
