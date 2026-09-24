# План 184.3 — evohime-eval, Benchmark Matrix и CI diagnostics

Статус: active implementation contract. Этап следует после overview 184.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Расширить fixture schema и evohime-eval: static/deterministic/explicit real; добавить report refs/counts/hard failure в BenchmarkAttempt; bounded CI reason codes.

## Зависимости

### Блокирующие

Блокирующие: этап 184.2, current fixture loader and benchmark aggregation.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Schema mismatch/unavailable остаётся typed; real не заменяется другими modes.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

CLI/CI matrix проверяет mode isolation, per-assertion output, baseline compatibility and redaction.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Old fixtures получают явную compatibility path/error; digest сохраняет provenance.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
