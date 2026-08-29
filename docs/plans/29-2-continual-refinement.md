# План 29.2 — Continual Refinement с evidence и approval: runtime-интеграция и recovery

Статус: этап 2 для [плана 29.0](./29-0-continual-refinement.md); после [плана 29.1](./29-1-continual-refinement.md).

## Цель

Провести «Continual Refinement с evidence и approval» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 29.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

Reflection и evaluation запускаются только после terminal task observation либо
явного user action; отдельный candidate не создаётся на каждом model turn.
Предварительные runtime surfaces: Core refinement service/handler, existing
memory API, `skill_registry` и versioned PromptRule adapter. Их имена уточняются
по live checkout до реализации.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot
   active candidate/activation attempt.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

### Runtime vertical slice

- Выполнить `observe → normalize → aggregate → deduplicate → evaluate →
  propose`, закрепляя immutable policy/evidence snapshot на candidate.
- Перед каждой activation/rollback повторно проверить owner scope, current
  policy, approval token (если требуется), expected revision, source validity,
  target availability и idempotency; mutation выполняется только через target
  adapter. Unknown/failed target оставляет candidate неактивным.
- Evaluation должен различать schema/limit, duplicate, conflict, security,
  insufficient evidence и behavior failure; слабый результат — `Proposed`, а
  не implicit approval. Observation после activation создаёт новую revision или
  candidate и не изменяет активную запись на месте.
- Для rollback сохранять before/after target revisions. После restart
  незавершённая activation получает `unknown`/`reconciliation_required` и не
  повторяет внешний эффект вслепую.
- Тесты: `crates/evohime-core/tests/refinement_recovery.rs` (имя проверить на
  evidence freeze) для duplicate delivery, stale policy/version, approval
  loss, source deletion, crash до/после target dispatch, restart, rollback,
  unavailable PromptRule adapter и capability-escalation attempts.

### Acceptance-to-runtime matrix

- `R29-C01` → bounded terminal observation, independent aggregation и no-op для
  одиночного эпизода.
- `R29-C03` → typed evaluation outcomes и source invalidation.
- `R29-C04`/`R29-C05` → revalidation перед effect, approval и unavailable.
- `R29-C06`/`R29-C08` → dispatch marker, recovery, rollback и fault injection.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.
- [ ] Reflection не запускает side effect, activation не повторяется вслепую,
  а policy/approval/evidence snapshot закреплён на candidate/activation attempt.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
