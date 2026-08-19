# Этап 03.2: Coordinator state machine

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этап 03.1 (typed report и `CorrelationContext`/`Provenance` с
`parent_sequence`, который валидируется при переходах) и существующие leases
и task graph.

Разблокирует: 03.3 и 03.4.

## Что этап отдаёт наружу

Явные состояния child task, bounded leases и restart recovery.

## Что уже есть в коде

Есть: `ChildLifecycleState` (`crates/evohime-core/src/child_runtime.rs:47-59`)
ровно в описанных ниже состояниях (плюс `TimedOut` и `Aborted`), проверяемые
переходы (`allowed_transition`, `child_runtime.rs:179-209`) и события
lifecycle с порядковым номером. `is_terminal` (`child_runtime.rs:167-177`)
включает `Accepted, Rejected, Failed, Cancelled, TimedOut, Aborted`;
`transition()` возвращает `Err(TerminalState)` из любого terminal состояния —
явного restart/resume выхода из terminal нет и не требуется (см. «Restart и
terminal состояния» ниже). `Aborted` объявлен, но сейчас недостижим через
`transition()` ни из одного состояния — 03.2 добавляет путь `Running →
Aborted` (coordinator abort) и `Validating → Aborted`.

Нет: восстановления только из durable checkpoint после restart с повторной
валидацией report/evidence, bounded дочерних leases и fan-in нескольких
отчётов, самой сущности lease (в кодовой базе нет `lease`/`heartbeat`
структуры — ближайший прецедент `receipt_approval_intents`, см. ниже),
таблицы checkpoint и константы `max_revisions`.

## Lease-механизм

Lease — bounded право дочернего процесса на владение задачей в текущей
revision, привязанное к паре часов (`crates/evohime-receipts` уже использует
этот паттерн в `receipt_approval_intents`: `created_wall_at_ms` +
`created_monotonic_ms`/`deadline_monotonic_ms` + `clock_boot_id`). Lease
переиспользует ту же пару часов вместо TTL-freshness из
`ResearchEvidence::is_fresh_at` (`research.rs:88-129`), потому что lease
должен переживать перезапуск процесса, а не только истекать по wall-clock.

- **Модель:** hybrid. Child обновляет lease активно (heartbeat) через
  дешёвый Core call при каждом успешном lifecycle event и не реже, чем раз в
  `lease_heartbeat_interval_ms` (по умолчанию 5000 мс, настраивается per-role
  budget из 03.1). Coordinator дополнительно верифицирует lease пассивно при
  каждом restart и при истечении `deadline_monotonic_ms`.
- **Поля lease** (хранятся в checkpoint, см. схему ниже):
  `lease_deadline_monotonic_ms`, `lease_created_monotonic_ms`,
  `lease_clock_boot_id`, `lease_holder_process_id`.
- **Живой lease** = `lease_deadline_monotonic_ms > now_monotonic_ms` **и**
  `lease_clock_boot_id == current_clock_boot_id`. Смена `clock_boot_id`
  (перезапуск runtime) сама по себе не убивает lease мгновенно — coordinator
  переносит monotonic-часы на новый boot по тому же механизму, что
  `receipt_approval_intents`, и только затем проверяет deadline; если
  перенести часы нельзя (отсутствует reference point), lease считается
  мёртвым консервативно.
- **Продление:** каждый heartbeat устанавливает новый
  `lease_deadline_monotonic_ms = now_monotonic_ms + lease_ttl_ms`
  (`lease_ttl_ms` по умолчанию 15000 мс, ≥ 3×
  `lease_heartbeat_interval_ms`). Продление не расширяет child budget/grants
  из 03.1.
- **Истечение без heartbeat:** coordinator помечает child `Failed` с
  `reason=lease_expired` при первой проверке после `deadline_monotonic_ms`
  (polling на каждом coordinator tick, не отдельный таймер-процесс).

## Схема checkpoint

Checkpoint — таблица SQLite, конвенции по образцу `receipt_actions`/
`receipt_approval_intents` (`crates/evohime-receipts/src/runtime.rs:378+`):
`TEXT` для id/enum с `CHECK`, `INTEGER` для `*_ms`, `BLOB`/`TEXT` для
сериализованных payload, `schema_version` в каждой строке.

```sql
CREATE TABLE IF NOT EXISTS coordinator_child_checkpoint (
  schema_version INTEGER NOT NULL DEFAULT 1,
  child_task_id TEXT PRIMARY KEY NOT NULL,
  parent_task_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN (
    'created','queued','running','validating',
    'waiting_parent_acceptance','accepted','rejected',
    'failed','cancelled','timed_out','aborted'
  )),
  failure_reason TEXT,
  report_json BLOB,
  evidence_locators_json BLOB,
  provenance_hashes_json BLOB,
  parent_sequence INTEGER NOT NULL,
  lease_deadline_monotonic_ms INTEGER,
  lease_created_monotonic_ms INTEGER,
  lease_clock_boot_id TEXT,
  lease_holder_process_id TEXT,
  last_transition_event TEXT NOT NULL,
  last_transition_at_ms INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL
);
```

`report_json`/`evidence_locators_json`/`provenance_hashes_json` хранят typed
payload из 03.1 (`ChildReport`, evidence locators, `Provenance` hashes) как
canonical JSON — та же canonicalization, что и receipt payload из 01.1, чтобы
хеши были воспроизводимы при повторной валидации. `parent_sequence`
копируется из `CorrelationContext`/`Provenance` (`child_contracts.rs:84-132,
375`) — используется coordinator для восстановления порядка событий
относительно родителя и как один из критериев fan-in tie-break (см. 03-0).

Запись выполняется в той же транзакции, что и lifecycle event, **после**
успешного применения перехода в памяти и **до** отправки события наружу
(sequencing: transition → checkpoint write (commit) → event emit). Если
процесс падает между commit и emit, событие для этого перехода будет emit-нуто
повторно при следующем coordinator tick (события идемпотентны по
`(child_task_id, revision, last_transition_event)` — получатель дедуплицирует
по этому ключу). Если процесс падает до commit, переход не применялся и после
restart coordinator повторяет попытку с последнего закоммиченного состояния —
транзакционность SQLite гарантирует, что частично применённого checkpoint не
бывает.

## Повторная валидация report/evidence после restart

При restart для каждого child в нетерминальном состоянии coordinator
выполняет, в этом порядке, до возобновления или перевода в `Failed`:

1. **Lease check.** Если lease не живой (см. выше) — сразу `Failed(reason=
   restart_no_live_lease)`, дальнейшие проверки не выполняются.
2. **Hash check.** Пересчитать canonical hash `report_json` и каждого
   evidence locator, сравнить с `provenance_hashes_json`. Несовпадение —
   `Failed(reason=restart_hash_mismatch)`.
3. **Correlation check.** `parent_sequence` и `child_task_id`/`revision` в
   checkpoint должны совпадать с последним известным родителю значением
   (родитель — источник истины по order). Несовпадение — `Failed(reason=
   restart_correlation_mismatch)`.
4. **Schema check.** `report_json` должен проходить ту же typed-валидацию,
   что и при первичном приёме в 03.1 (schema version, обязательные поля).
   Несовпадение — `Failed(reason=restart_schema_invalid)`.

Валидация полная (все четыре шага для каждого поля checkpoint), частичной
ревалидации нет — child либо целиком доверенный после restart, либо
переводится в `Failed`. Успешное прохождение всех шагов возобновляет child в
зафиксированном `state` без создания нового перехода (restart не является
lifecycle event).

## Restart-переходы и terminal состояния

Restart добавляет в state machine только переходы из нетерминальных
состояний в `Failed`; они проверяются `allowed_transition` наравне с обычными
переходами:

- `Running → Failed` (reason ∈ `restart_no_live_lease`,
  `restart_hash_mismatch`, `restart_correlation_mismatch`,
  `restart_schema_invalid`);
- `Validating → Failed` (те же reasons);
- `WaitingParentAcceptance → Failed` (те же reasons; report уже прошёл
  Validating, поэтому здесь чаще срабатывает только `restart_no_live_lease`).

`TimedOut` и `Aborted` уже terminal — restart не создаёт для них переход,
только идемпотентный cleanup (см. ниже): lease, если ещё числится активным в
checkpoint, освобождается, но `state` не меняется. `Queued`/`Created` без
активного lease при restart остаются как есть (ничего не выполнялось,
восстанавливать нечего) и планируются заново обычным путём.

## Идемпотентный cleanup lease/процесса

Переиспользует паттерн `ApprovalGC` (`crates/evohime-receipts/src/
runtime.rs:1077-1097`, `spawn_approval_gc` в `crates/evohime-core/
src/lib.rs:3493-3511`): фоновый цикл на интервале (по умолчанию 60 с),
каждый tick — одна транзакция, которая:

1. перечитывает `receipt_runtime_guard`/эквивалентный coordinator guard —
   no-op, если recovery ещё выполняется;
2. находит checkpoint-строки в terminal состоянии
   (`Failed|Rejected|Cancelled|TimedOut|Aborted`) с непустым
   `lease_holder_process_id`;
3. посылает cleanup сигнал процессу/lease-хранилищу только если процесс
   действительно жив (`lease_holder_process_id` не отвечает → уже нечего
   останавливать) и снимает `lease_*` поля в той же транзакции;
4. коммитит.

Условие в шаге 3 делает операцию идемпотентной: повторный вызов на уже
очищенной строке не находит `lease_holder_process_id` и ничего не делает —
такое же no-op поведение, как у `ApprovalGC` с уже удалёнными строками.

## Retry/revision-лимиты

`max_revisions` — константа из 03.1 (`child_contracts.rs:491,528,614-617`),
которую 03.1 обязан завести как default (2, абсолютный максимум 3); 03.2 не
переопределяет число, только использует его в переходах:

- `max_revisions` считает только revision, порождённые reviewer `revise`
  (см. 03-0), не restart-переходы;
- restart не создаёт новую revision и не расходует лимит — это то же
  число попыток, к которому coordinator возвращается после lease/hash
  ревалидации;
- после исчерпания `max_revisions` действует правило 03-0: `revise_plan`,
  если изменились предпосылки/границы, иначе `Failed(reason=
  max_revisions_exceeded)`; новый implementer автоматически не создаётся.

## Fan-in конфликты

Правила упорядочивания уже заданы в [03-0, раздел «Параллельное
исследование»](03-0-specialized-child-workflows.md#параллельное-исследование):
свежий published source с валидной provenance → более специфичный
path/chunk scope → меньший `parent_sequence` → лексикографический
`content_hash`. 03.2 конкретизирует, что считается конфликтом и кто его
разрешает:

- **Конфликт** — два или более evidence locator с пересекающимся
  path/chunk scope и несовпадающим `content_hash`, полученные от разных
  child в одной fan-in группе.
- **Разрешитель** — coordinator, детерминированно, по правилам 03-0; человек
  не участвует в обычном случае.
- **Итог разрешения** — ровно один evidence locator выбирается победителем
  на пересекающийся scope, остальные помечаются `superseded` в checkpoint
  (`evidence_locators_json`) с полем `superseded_by` и причиной
  (`fresher_source|more_specific_scope|lower_parent_sequence|hash_tiebreak`).
- **Неразрешённый случай** — если после всех критериев остаётся более
  одного кандидата (полностью идентичные tie-break значения), спор попадает
  в `unknowns` и **блокирует implementer** до explicit coordinator approval
  (не автоматический выбор) — это тот случай, где 03-0 требует ручной
  разбор.
- Выбранные evidence, отклонённые конфликтующие кандидаты и причина выбора
  входят в checkpoint и trace для каждого fan-in.

## Partial tester failure

Обязательный (required) acceptance criterion — помеченный так coordinator
при создании child (см. 03-0, «Контракт child task»); необязательный —
не помеченный. Провал обязательного → весь child получает `revise`. Провал
только необязательных → результат может быть `Accepted`, но только со
списком `risks[]` (уже часть отчёта из 03-0) и явным coordinator approval
(отдельное решение, не побочный эффект приёма report); без approval —
`WaitingParentAcceptance` остаётся open до решения.

## Observability

Coordinator публикует per-child метрики через существующий event journal
(не отдельная подсистема): длительность в каждом состоянии, причина перехода
в `Failed` (`reason` из checkpoint), число restart-ревалидаций и их исход,
lease renewal latency. Эти поля уже есть в checkpoint/trace — 03.4 строит
UI/observability поверх них, 03.2 только гарантирует, что они пишутся при
каждом переходе.

## Содержание

- Зафиксировать Created → Queued → Running → Validating →
  WaitingParentAcceptance → Accepted/Rejected/Failed/Cancelled, плюс
  `Running/Validating → Aborted` (coordinator abort) и `Running → TimedOut`
  (уже есть).
- Не считать child success финальным task success.
- Дочерние leases, cancellation и restart recovery — bounded по правилам
  выше (lease TTL, max_revisions, идемпотентный cleanup).
- После restart coordinator восстанавливает только durable checkpoint
  (`coordinator_child_checkpoint`) и повторно валидирует report/evidence по
  четырём шагам выше. Checkpoint — атомарная SQLite-запись в одной
  транзакции с lifecycle event.
- После restart `Running`, `Validating` и `WaitingParentAcceptance` без
  подтверждённого живого lease помечаются `Failed` с конкретным `reason`
  (см. «Restart-переходы»), а cleanup lease/process выполняется идемпотентно
  по паттерну `ApprovalGC`.
- Reviewer `revise` содержит evidence и список нарушенных acceptance
  criteria. Coordinator создаёт новую revision только в пределах
  `max_revisions`; после лимита действует правило из 03-0.
- Fan-in выполняется до implementer по правилам 03-0 и разрешению конфликтов
  выше; выбранные evidence, конфликты и причины выбора входят в checkpoint и
  trace.

## Проверки

- sequential, concurrent, conditional workflow fixtures;
- lease liveness: heartbeat продлевает deadline, отсутствие heartbeat в
  течение `lease_ttl_ms` переводит child в `Failed(lease_expired)`;
- cancellation/restart/lease-loss recovery для каждого из четырёх
  restart-reason (`no_live_lease`, `hash_mismatch`, `correlation_mismatch`,
  `schema_invalid`);
- checkpoint round-trip: crash между commit checkpoint и event emit не
  теряет и не дублирует терминальный исход (дедуп по
  `(child_task_id, revision, last_transition_event)`);
- reviewer rejection → bounded revision, restart не расходует
  `max_revisions`;
- fan-in deterministic ordering, conflict marking (`superseded_by`) и
  unresolved-conflict эскалация в `unknowns`;
- partial tester failure: обязательный criterion → revise, необязательный →
  Accepted только с явным coordinator approval;
- restart cleanup без orphan leases/processes — повторный GC-тик на уже
  очищенной строке не делает ничего (идемпотентность).

## Критерии готовности

- parent никогда не принимает child result без validation;
- restart/cancellation не оставляют orphan processes or leases;
- lease-механизм детерминированно отличает живой child от мёртвого после
  restart, включая смену `clock_boot_id`;
- fan-in конфликты разрешаются детерминированно или явно эскалируются, без
  автоматического выбора при неразрешимой tie-break ситуации.
