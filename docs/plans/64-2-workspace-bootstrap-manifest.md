# План 64.2 — Workspace Bootstrap Manifest: безопасная подготовка project environment перед agent run: runtime-интеграция и recovery

Статус: этап 2 для [плана 64.0](./64-0-workspace-bootstrap-manifest.md); после [плана 64.1](./64-1-workspace-bootstrap-manifest.md).

## Цель

Провести «Workspace Bootstrap Manifest: безопасная подготовка project environment перед agent run» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 64.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 41.0 — зависимость из обзора.
- План 47.0 — зависимость из обзора.
- Tool Simulation Runtime v1 из `../architecture.md`.
- Канонический раздел `architecture.md` — Agentic Browser Session v1.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/workspace_bootstrap_manifest.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `WorkspaceBootstrapManifestService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/workspace_bootstrap_manifest_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть versioned WorkspaceBootstrapManifest. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C03` — Все commands запускаются через ExecutionPolicy. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Есть fingerprint/freshness cache, чтобы не запускать setup каждый turn. → провести через typed outcome, timeout, cancellation и idempotency.
- `C07` — Tool/dependency checks отделены от auto-install. → провести через typed outcome, timeout, cancellation и idempotency.

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
