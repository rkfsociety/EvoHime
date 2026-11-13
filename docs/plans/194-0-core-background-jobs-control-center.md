# План 194.0 — Core-owned Background Jobs Control Center

Статус: active implementation contract. План развивает существующий background-execution/v1 и desktop projections; он не создаёт новый job runtime.

## Цель
Сделать длительные и detached задачи Евы управляемыми: единый центр текущих/ожидающих/завершённых задач, прогресс, источник запуска, зависимости, результаты, ошибки, отмена и восстановление после перезапуска. Пользователь должен различать выполнение, ожидание ресурса, ожидание approval, partial result и неизвестный исход.

## Основание checkout
Plan 132 уже реализовал Core-owned background-execution/v1, schema v103, bounded queues/waits/wakeups/attempt history, restart reconciliation, generation-fenced wake transitions и authenticated IPC command 262/event 107. Shell получает redacted projection, но отдельного Background Execution UI нет. OperationsPanel уже показывает часть child jobs и Pulse. План закрывает UX/операционные пробелы поверх действующих owners.

## Граница
Core остаётся владельцем lifecycle, status, scheduling, retry, cancellation, leases, persistence и recovery. Electron получает только bounded redacted projection и отправляет typed user intent через authenticated IPC. Используются existing automation/workflow/agent/goal/human-work/remote-task owners и план 132. Новый scheduler, job table, retry loop или side-effect executor запрещён.

## Изменяемые контракты
- Расширение background-execution/v1 projection только при доказанном отсутствии нужного metadata.
- Typed commands для разрешённых user actions: cancel, open result/evidence; pause/resume только если семантика уже поддерживается Core owner; retry только при доказанной идемпотентности и известном исходе.
- UI model: status, origin, task/owner, progress evidence, dependencies, next wake, attempt history, error/recovery state и artifact refs.
- Retention, pagination, sorting и filters остаются bounded.

## Зависимости
### Блокирующие
Plan 132; workflow runtime, automation, TaskCoordinator, resource admission, task lease/fencing и approval; ArtifactStore/provenance и authenticated desktop IPC.
### Опциональные
План 193 даёт richer agent/child assignment context; план 195 может добавить workflow recipe metadata.

## Recovery и rollback
Missing adapter не считается успешным completion. Stale snapshots сверяются по revision/generation; после reconnect загружается новый snapshot, events deduplicated. Retry запрещён при UnknownOutcome и разрешается только через существующий owner с доказанной идемпотентностью. Rollback удаляет UI/projections и возвращает текущие OperationsPanel surfaces; runtime и данные неизменны.

## Этапы
- [Этап 1 — projection и lifecycle action contract](./194-1-core-background-jobs-control-center.md)
- [Этап 2 — Core integration, recovery и bounded controls](./194-2-core-background-jobs-control-center.md)
- [Этап 3 — IPC и Background Jobs UI](./194-3-core-background-jobs-control-center.md)
- [Этап 4 — verification, release evidence и closure](./194-4-core-background-jobs-control-center.md)

## Критерии готовности
- [ ] Все job types представлены единым projection без объединения разных lifecycle owners.
- [ ] UI различает queued/running/waiting/paused/partial/failed/completed/cancelled/unknown.
- [ ] Прогресс grounded на observed events/attempts, а не на model-generated percentages.
- [ ] Cancellation и controls выполняются только владельцем задачи через typed command.
- [ ] Retry не повторяет ambiguous side effects.
- [ ] Snapshot/events bounded, paginated, revision-safe и redacted.
- [ ] Recovery, accessibility и reconnect behavior подтверждены тестами.

## Non-goals
Второй scheduler/job database/runtime; произвольный restart/retry любого job из UI; автоматическое approval опасных действий; UI-доступ к SQLite/workspace/provider credentials; отдельный web dashboard или remote control plane.

## Verification и release evidence
Contract/projection tests → lifecycle/cancel/recovery integration → IPC compatibility → renderer state matrix/accessibility → CI and Windows acceptance. Сохранять projection version, command/event tags, bounded job counts, status-transition tests, cancellation/recovery results, redaction checks и focused CI links. Не сохранять job payloads, secrets или full logs.

## Критерии выхода overview
- [ ] Background execution остаётся single Core-owned lifecycle.
- [ ] UI является projection, а не alternate scheduler/authority.
- [ ] Каждый control имеет typed authorization, idempotency и recovery semantics.
