# План 186.2 — verified local inference execution

Статус: active implementation contract. Этап следует после overview 186.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Добавить side-effect-free adapter behind scheduler, verified artifact/session, bounded framed protocol, validation and cancellation. No renderer-supplied executable path/argv.

## Зависимости

### Блокирующие

Блокирующие: 186.1, scheduler, supervisor/job lifecycle and verified artifact promotion.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Before dispatch can requeue if deadline valid; interrupted pure inference requires new attempt with same frozen hashes; malformed output not cached.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Adapter fixture integration: unsupported/bad frame/timeout/cancel/wrong hash/oversize/unavailable.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Windows-capable backend без mandatory Python/CUDA/network; новая production dependency остаётся отдельным approval blocker.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
