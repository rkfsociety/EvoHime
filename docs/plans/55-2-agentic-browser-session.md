# План 55.2 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой: runtime-интеграция и recovery

Статус: этап 2 для [плана 55.0](./55-0-agentic-browser-session.md); после [плана 55.1](./55-1-agentic-browser-session.md).

## Цель

Провести «Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 55.1 — contract, validators, storage policy и errors.
- Existing browser-session tools are migrated atomically or disabled; no
  legacy selector/CDP bypass remains registered.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Execution Policy Profiles adapter из overview.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Core service owns lifecycle/ref/policy state; packaged browser adapter owns
  engine mechanics only and accepts bounded typed commands, never raw CDP from
  model/renderer.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/agentic_browser_session_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Модель работает через typed browser tools и stable element refs. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Default browser profile isolated/ephemeral. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C06` — Upload/download проходят Artifact/Core boundaries. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- Navigation validates every redirect and resolved destination. Page revision
  changes invalidate element refs; click/type require exact session/page/ref
  preconditions. Human takeover acquires an exclusive lease and fences agent
  actions until explicit release.
- Backend launch must be owned by Core/supervisor and the packaged executable
  must be present in native package manifest and smoke fixture. An env CDP
  endpoint is test-only and may not satisfy production acceptance.
- Browser network policy must be enforced at the actual backend connection
  boundary (proxy/interception or equivalent), including redirect and DNS
  rebinding checks; preflight URL validation alone is not evidence.

### Recovery contract

- Durable transitions восстанавливаются replay/reconciliation; transient work после restart получает typed `unknown`/`unavailable`, а не повтор side effect.
- Fault injection должна доказать отсутствие duplicate effect, потерю approval, обход policy или расширение capability set.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.
- [ ] Browser process cleanup leaves no profile/process leak; crash after
  dispatch produces `unknown_outcome` and requires explicit reconciliation.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
