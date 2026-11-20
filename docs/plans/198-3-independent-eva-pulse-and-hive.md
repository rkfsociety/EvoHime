# План 198.3 — IPC, projections и UI Улья

## Текущий прогресс

Добавлены additive `EvaPulseStatusCommand`/`eva_pulse.status`, Electron bridge
и defensive parser в `OperationsPanel`. UI предпочитает независимую Pulse
projection, но имеет bounded fallback на legacy-поля `hive_state.snapshot` для
совместимости со старым Core.
В schema v7 новые Hive snapshots перестают сериализовать legacy Pulse-поля;
старые schema 1–6 продолжают читаться через fallback.

## UI-контракт

- Улей показывает Королеву Еву как центральный runtime и её внутреннюю работу.
- Пользовательские подписи не используют `EvaPulse` как режим Улья.
- Отдельный диагностический статус EvaPulse не смешивается с `queen.status`,
  `active_agents`, `active_tasks` или Hive approvals.
- Approval с `origin = eva_pulse`, если Королева приняла сигнал и создала Hive
  task, появляется в Улье с task/origin/correlation metadata.

## Работа

1. Обновить canonical proto и generated Rust/TypeScript types.
2. Добавить authenticated commands/events для signal submit, outcome и
   independent status.
3. Исправить conversation/event projection: approval не должен зависеть только
   от наличия записи в `hive-system-chat`; Core отдаёт bounded Hive approval
   projection.
4. Обновить `OperationsPanel`, navigation badge, Queen activity и logs.
5. Показывать source/origin без raw prompt, credentials или лишнего payload.

## Verification

- protocol compatibility and `npm run check:protocol`;
- Hive approval E2E: pending → allow/reject → terminal;
- approval from external EvaPulse signal is visible only after Hive acceptance;
- ordinary chat approval не появляется в Улье;
- accessibility and renderer security tests.
