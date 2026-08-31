# План 53.2 — Diagnostics & Support Bundle: redacted health snapshot и воспроизводимый issue draft: runtime-интеграция и recovery

Статус: этап 2 для [плана 53.0](./53-0-diagnostics-and-support-bundle.md); после [плана 53.1](./53-1-diagnostics-and-support-bundle.md).

## Цель

Провести «Diagnostics & Support Bundle: redacted health snapshot и воспроизводимый issue draft» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 53.1 — contract, validators, storage policy и errors.
- Existing shell bundle assembler and Core health/run snapshot providers.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Goal/TaskCheckpoint selected-run summaries.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Core snapshot handler выполняет bounded health/run collection; Electron main
  assembler получает только typed/redacted sections, добавляет shell-owned
  sections, строит preview, issue draft и deterministic manifest.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Перед user-selected save выполнить второй scan всего текстового archive;
  unknown/blocked section перечислить в manifest. Temporary archive создаётся
  с restrictive ACL и удаляется при cancel/failure; network запрещён тестом.
- Тесты: `crates/evohime-core/tests/diagnostics_and_support_bundle_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C06` — Можно собрать контекст конкретного failed run. → провести через typed outcome, timeout, cancellation и idempotency.

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
