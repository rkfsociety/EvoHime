# План 35.2 — Invocation Presets: version-pinned шаблоны запусков без копирования секретов: runtime-интеграция и recovery

Статус: этап 2 для [плана 35.0](./35-0-invocation-presets.md); после [плана 35.1](./35-1-invocation-presets.md).

## Цель

Провести «Invocation Presets: version-pinned шаблоны запусков без копирования секретов» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 35.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 33.0 — зависимость из обзора.
- План 30.0 — optional portable export/import; local preset run и scheduler
  работают без него.
- План 34.0 — optional trigger base mapping; без него event fields остаются
  единственным trigger input source.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces.
   Preset resolve идёт через обычный workflow runtime; optional package/trigger
   integration даёт typed unavailable/degraded и не блокирует manual/schedule run.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/invocation_presets.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `InvocationPresetsService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для completed run использовать только Core-owned invocation metadata через
  deterministic sanitizer; preview до сохранения и повторная schema/credential
  validation обязательны.
- Для migration сравнить pinned definition/schema hashes, выполнить только
  explicit compatible mapping или вернуть typed `NeedsMigration`/
  `IncompatibleSchema`; запись preset и migration result атомарны.
- Schedule запускается с immutable preset revision/hash snapshot; последующее
  редактирование preset не меняет уже подтверждённый schedule.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/invocation_presets_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть durable InvocationPreset contract. → журналировать переходы и восстановление через replay/reconciliation.
- `C02` — Preset pinned к workflow version. → провести через typed outcome, timeout, cancellation и idempotency.
- `C03` — Можно создать preset из completed run. → провести через typed outcome, timeout, cancellation и idempotency.
- `C06` — Есть migration flow между workflow versions. → провести через typed outcome, timeout, cancellation и idempotency.
- `C07` — Preset запускается через обычный workflow runtime. → провести через typed outcome, timeout, cancellation и idempotency.
- `C08` — Preset можно использовать scheduler без обхода approvals. → закрепить revision/hash snapshot и прогнать обычный Core policy/approval path.
- `C10` — Удалённый/expired credential даёт `NeedsRebinding`. → остановить dispatch до явного rebinding.
- `C12` — Schedule фиксирует revision/hash snapshot. → проверить drift после редактирования preset.
- `C13` — Version drift показывает preview и не выполняет silent migration. → вернуть typed migration outcome до запуска.
- `C14` — Trigger base configuration optional и не меняет protected identities. → при отсутствии плана 34 использовать typed unavailable и event-only mapping.

### Recovery contract

- Durable transitions восстанавливаются replay/reconciliation; transient work после restart получает typed `unknown`/`unavailable`, а не повтор side effect.
- Fault injection должна доказать отсутствие duplicate effect, потерю approval, обход policy или расширение capability set.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
