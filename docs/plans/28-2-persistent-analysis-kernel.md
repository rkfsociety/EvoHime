# План 28.2 — Persistent Analysis Kernel: runtime-интеграция и recovery

Статус: этап 2 для [плана 28.0](./28-0-persistent-analysis-kernel.md); после [плана 28.1](./28-1-persistent-analysis-kernel.md).

## Цель

Провести «Persistent Analysis Kernel» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 28.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.
- Supervisor launch/recovery path и действующие Job Object/resource-limit
  primitives; новый worker не должен обходить этот lifecycle.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Загрузить stage-1 artifacts, schema/hash и закрепить immutable
   contract/policy/grant snapshot для active run; проверить child refs против
   plan-27 allowlist.
1. Добавить Core handler/state machine и supervisor-managed worker/process.
   Launch command принимает только фиксированный runtime/package identity и
   bounded manifest, не executable или arguments от model/renderer; supervisor
   создаёт отдельный Job Object, применяет limits, whitelist environment и
   отсутствие наследуемых credentials/лишних handles, затем возвращает typed
   launch outcome. Повторить authorization непосредственно перед host effect и
   связать result/event с correlation + idempotency.
2. Ограничить worker environment/handles/working directory и ресурсы явными
   defaults (CPU/time, memory, output, object count/size, request rate,
   idle/lifetime); при превышении — typed limit и hard reset. Не использовать
   незафиксированные OS sandbox assumptions.
3. Подключить только заявленные registry/workflow/child/provider/tool surfaces.
   Optional Goal/Continuation/backend даёт typed `unavailable`/`degraded` и не
   меняет state/authority.
4. Формализовать timeout, lease, retry, backpressure, partial failure,
   cancellation и unknown outcome; dispatch marker до host effect, после
   restart только replay/reconciliation, без blind retry.
5. Сделать fault-injection для crash до/после marker, stale version/lease,
   duplicate delivery, policy change, object corruption, direct FS/network/
   shell/credential attempts и worker escape.
6. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.
- [ ] Worker действительно запускается через allowlisted supervisor/Core
  lifecycle и отдельный Job Object; resource breach приводит к reset, а
  подмена runtime/manifest и direct FS/network/shell/credential attempts дают
  отказ без side effect.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
