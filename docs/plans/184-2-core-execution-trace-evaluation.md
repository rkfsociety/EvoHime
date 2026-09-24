# План 184.2 — Core capture, evaluation lifecycle и recovery

Статус: active implementation contract. Этап следует после overview 184.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Подключить existing journal/ledger events к Core normalizer, partition по run/attempt, freeze trace и запускать pure evaluator; report связать с trace+contract hashes.

## Зависимости

### Блокирующие

Блокирующие: этап 184.1, sequence, benchmark attempt identity, cancellation/recovery.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Frozen trace переоценивается идемпотентно; partial trace remains Interrupted; evaluator error typed failure; no unknown-effect retry.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Integration: observed/wrong sequence/args, approval denied then effect, retries, cancellation, report idempotency, child partition.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Verdict вычисляется только из observed normalized trace и не изменяет policy.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
