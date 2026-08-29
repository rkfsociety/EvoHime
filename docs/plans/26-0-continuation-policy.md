# План 26.0 — Continuation Policy и quality gates

Статус: предложено по [issue #6](https://github.com/rkfsociety/EvoHime/issues/6).
Policy — отдельный Core-owned decision layer для bounded продолжения после
turn/workflow result; она не превращается в daemon или shell-интерпретатор.

## Цель

Разрешить контролируемый цикл `изменить → проверить → исправить → проверить`
без ручного «продолжай» на каждом промежуточном результате. Решение
`Continue|Complete|PauseForApproval|Blocked|BudgetLimited|StopFailed|StopUser`
принимается Core на основе typed evidence, immutable policy snapshot, budgets и
quality gates. Модель может предложить intent, но не может продолжить сама.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./26-1-continuation-policy.md)
- [Этап 2 — runtime-интеграция и recovery](./26-2-continuation-policy.md)
- [Этап 3 — IPC, client projection и UI](./26-3-continuation-policy.md)
- [Этап 4 — verification, release-evidence и закрытие](./26-4-continuation-policy.md)

## Зависимости

### Блокирующие

- план 23 TaskCheckpoint для continuity, attempt fingerprint и recovery;
- план 25 Persistent Goals для authoritative completion criteria;
- существующие workflow/tool registry, approval registry, leases и unknown
  outcome handling;
- durable SQLite/event journal и authenticated IPC.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Контракт

Ввести `ContinuationPolicyV1`: `id`, `version`, `scope`, `enabled`, optional
`linked_goal_id`, `max_continuations`, `max_model_turns`, token/cost/wall-clock
budgets, `require_workspace_change_before_retry`, `stop_on_user_interaction`,
`stop_on_approval_required`, `stop_on_unknown_outcome`, `gates[]`,
`completion_mode`, timestamps и hash.

`scope` обязан включать стабильный workspace/owner scope и actor, а policy не
может читать или изменять состояние другого scope. Для каждого run отдельно
фиксируются `run_id`, `policy_snapshot_hash`, `goal_version`, capability и
approval snapshot. `enabled` разрешает только явно созданный пользователем
bounded run; он не является глобальным переключателем daemon.

`GateV1` содержит `id`, `kind` (`tool|workflow|evidence|approval`), typed
`capability_ref`, bounded args, required status, timeout и retry policy. В policy
нельзя хранить произвольную shell string. Gate разрешается только через
существующий registry и теми же approvals/grants.

`args` — discriminated typed payload, а не произвольный JSON: canonical args
hash и sensitivity label входят в identity gate. Результат gate обязан иметь
revision/freshness, observed status и evidence/provenance ref. Policy не
выбирает произвольный следующий effect: typed continuation request приходит
из Core workflow/runner, а model intent только предлагает его и проходит ту же
валидацию.

При каждом решении Core собирает goal status, remaining criteria, gate results,
workspace/evidence change marker, failure class, retryability, approvals,
budgets, child/workflow state и recovery status. Required gates must pass before
`Complete`; unknown outcome, failed required gate или unresolved blocker не
маскируются под успех.

## Anti-loop и immutable snapshot

Попытка получает fingerprint из policy version, gate id, relevant workspace or
evidence hash и canonical args. Один и тот же failed fingerprint не повторяется
бесконечно: учитываются лимит повторов, отсутствие прогресса N cycles, backoff
для transient errors и немедленная остановка non-retryable errors. `unknown_outcome`
не повторяется слепо.

На старте autonomous run фиксируются policy hash/version, gate definitions,
Goal version, capability bindings и budgets. Изменение global policy не меняет
уже запущенный run. Пользовательский stop имеет приоритет и переживает restart.

Нужно различать `user_stop`, `user_pause`, `approval_resolution` и обычное
изменение UI. `stop_on_user_interaction` не должен блокировать саму команду
resume/approval; после stop новый run получает новый identity, а старый не
возобновляется.

## Persistence и recovery

Хранить continuation index, start time, budget counters, progress marker, gate
history, fingerprints, last checkpoint и stop reason в additive durable store.
После restart сначала восстанавливается workflow/lease state и unknown outcome,
затем policy решает, есть ли безопасный следующий шаг. Approval для новых args,
hash или capability не наследуется от старого решения.

Budget accounting должен быть атомарным: перед dispatch Core резервирует
bounded единицы (turns/tokens/cost/time), после результата коммитит фактическое
потребление или освобождает резерв. Concurrent/duplicate delivery не может
увеличить счётчик дважды; переполнение и отрицательные значения дают typed
`BudgetLimited`/`InvalidBudget`.

## UI и IPC

В Goal/Operations projection показывать enabled state, iteration/limit,
token/cost/time budgets, gates и последний result, причину Continue/Stop,
pause/stop/resume и pending approval. Команды pause/stop являются отдельными
идемпотентными user actions; renderer не изменяет policy snapshot или counters.

## Этапы реализации

1. Зафиксировать decision table, policy/gate schema, fingerprint и stop reasons.
2. Добавить durable run/policy snapshot store и transactional budget accounting.
3. Подключить Core decision point после turn/workflow result, approval pause и
   Goal completion proof.
4. Реализовать anti-loop, backoff, stale event protection и recovery.
5. Добавить bounded IPC/UI и evaluation fixtures для успешных/остановленных
   циклов.

## Предметная декомпозиция

- Core contract: `crates/evohime-core/src/continuation.rs` (или согласованный
  live-path), с регистрацией в `lib.rs`; typed policy, gate, decision,
  continuation request, snapshot и stable errors.
- Storage: `crates/evohime-local-storage/src/continuation_store.rs` и shared
  migration ladder; текущий checkout имеет schema v33, поэтому первый вариант
  обязан явно выбрать additive v34 либо доказать отсутствие новой durable
  таблицы. Run, snapshot, budget reservation, gate history и dedup пишутся
  транзакционно, с backup-before-migrate.
- Runtime: существующие `workflow_runtime.rs`, `workflow_runner.rs`,
  `child_workflow.rs`, `goal.rs` и model-dispatch/provenance path; continuation
  не создаёт второй lease, approval или effect ledger.
- IPC/UI: additive messages в
  `crates/desktop-ipc/proto/evohime.desktop.proto`, адаптеры
  `desktop/evohime-electron/src/main/ipc/pipe-client.ts` и
  `shell-bridge.ts`, projection в `GoalPanel`, `WorkflowPanel` и
  `OperationsPanel`. Renderer только отображает Core projection.

## Acceptance-to-contract matrix

- `C01` — Core выбирает decision по typed evidence → decision table и
  deterministic fixture для каждого terminal/continue outcome.
- `C02` — Complete требует Goal criteria и required gates → Core verifier refs,
  freshness и negative fixture с failed/unknown gate.
- `C03` — Нет capability escalation или shell execution → typed gate union,
  registry revalidation и fixture с shell-like аргументом.
- `C04` — Нет бессмысленных retries → canonical attempt identity,
  no-progress threshold, backoff и unknown-outcome fixture.
- `C05` — Restart безопасен → transaction boundaries, dispatch marker,
  reservation/dedup rules и crash injection до/после effect.
- `C06` — User stop имеет приоритет → durable stop action, stale resume denial
  и fixture, показывающий, что stop не создаёт новый effect.
- `C07` — Client показывает фактическое состояние → additive projection,
  replay/resync и redaction fixture.

## Критерии готовности

- Continue/Stop выбирает Core по typed evidence, не свободный текст модели;
- required gates и Goal criteria обязательны для Complete;
- grants/approvals не расширяются, shell strings не выполняются из policy;
- fingerprint/no-progress/unknown outcome предотвращают бессмысленные retries;
- counters, snapshot, stop reason и pending approval переживают restart;
- user stop немедленно блокирует новые continuations;
- UI объясняет каждое продолжение и остановку;
- проходят tests на retryable/non-retryable, budgets, approval, stale event,
  immutable snapshot и recovery.

## Не входит

Бесконечный daemon, автоматический запуск без explicit enable/policy, новый
scheduler, произвольные shell-команды и обход user control.
