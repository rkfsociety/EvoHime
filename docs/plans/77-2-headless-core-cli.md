# План 77.2 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation: runtime-интеграция и recovery

Статус: этап 2 для [плана 77.0](./77-0-headless-core-cli.md); после [плана 77.1](./77-1-headless-core-cli.md).

## Цель

Провести «Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 77.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 30.0 — зависимость из обзора.
- План 31.0 — зависимость из обзора.
- Реализованный Conversational Workflow Composer v1 — зависимость из обзора.
- План 59.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/headless_core_cli.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `HeadlessCoreCliService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/headless_core_cli_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C04` — Есть stable run ids, watch/cancel/resume/status. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Non-interactive approval semantics fail-safe. → повторить authorization непосредственно перед dispatch/effect.

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
