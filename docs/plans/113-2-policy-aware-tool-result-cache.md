# План 113.2 — Policy-Aware Tool Result Cache: freshness, provenance и safe reuse read-only calls: runtime-интеграция и recovery

Статус: этап 2 для [плана 113.0](./113-0-policy-aware-tool-result-cache.md); после [плана 113.1](./113-1-policy-aware-tool-result-cache.md).

## Цель

Провести «Policy-Aware Tool Result Cache: freshness, provenance и safe reuse read-only calls» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 113.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 38.0 — зависимость из обзора.
- План 40.0 — зависимость из обзора.
- План 60.0 — зависимость из обзора.
- План 105.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/policy_aware_tool_result_cache.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `PolicyAwareToolResultCacheService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/policy_aware_tool_result_cache_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Default cacheability = Never. → провести через typed outcome, timeout, cancellation и idempotency.
- `C03` — Cache key учитывает version/schema/resource/account/policy context. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Cached results сохраняют source provenance/observed time. → провести через typed outcome, timeout, cancellation и idempotency.
- `C06` — Mutating tools не используют result cache в MVP. → провести через typed outcome, timeout, cancellation и idempotency.
- `C07` — Workspace/provider/credential drift инвалидирует entries. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C08` — Sensitive cache storage регулируется policy. → провести через typed outcome, timeout, cancellation и idempotency.

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
