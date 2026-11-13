# План 194.4 — verification, release evidence и закрытие

Статус: active implementation contract. Этап следует после 194.1–194.3.

## Scope и изменяемые контракты
1. Проверить matrix всех background owners: automation, workflow, agent, goal, human-work и remote task; unknown optional adapters остаются unavailable.
2. Проверить lifecycle transitions, cancel races, restart recovery, generation fencing, stale revisions, pagination и event replay.
3. Проверить redaction/security: no raw payloads/prompts/secrets/absolute paths; no retry of UnknownOutcome; no UI bypass of approval/lease.
4. Проверить UI states, accessibility, keyboard interaction, responsive layout, reconnect and empty/error conditions.
5. Обновить architecture/current-state/release-evidence/plans README по подтверждённому поведению; удалить временные plan files 194 после closure.

## Зависимости
### Блокирующие
Завершённые 194.1–194.3, focused Rust/IPC/renderer tests и CI evidence.
### Опциональные
План 193 assignment metadata; не блокирует общий job control center.

## Recovery и rollback
План остаётся active при любом неподтверждённом lifecycle/security criterion. Rollback отключает новую UI surface и additive IPC, сохраняя schema v103 и background execution. Не удалять job history и не отменять tasks при rollback.

## Verification
Focused contract/lifecycle tests → restart/cancel fault matrix → IPC compatibility → renderer/accessibility matrix → CI and Windows acceptance. Тяжёлые локальные команды не запускать без прямой просьбы в соответствии с AGENTS.md.

## Release evidence
Projection/IPC versions, supported owner matrix, transition/cancel/recovery results, redaction checks, accessibility and renderer CI results, rollback verification. Evidence должна быть redacted.

## Критерии выхода
- [ ] Existing background execution owners remain authoritative.
- [ ] UI status/progress/control behavior is evidence-backed.
- [ ] Cancellation/recovery and unknown outcomes are safe.
- [ ] Focused CI gates pass; canonical docs updated; plan files removed.
