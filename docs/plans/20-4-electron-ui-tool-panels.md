# План 20.4 — панели инструментов и «Слух»

Статус: готов к реализации после завершения 20.3.

## Цель

Привести Overview, Plan Review, Operations, Workflow и Listening к общей
визуальной системе, сохранив все существующие действия, очереди, формы,
долгие операции и destructive confirmations.

## Зависимости

### Блокирующие

- `20-1-electron-ui-shell-tokens.md`;
- `20-2-electron-ui-projects-and-account-menu.md`;
- `20-3-electron-ui-chat-surface.md`;
- текущие `OverviewPanel.tsx`, `PlanReviewPanel.tsx`, `OperationsPanel.tsx`,
  `WorkflowPanel.tsx`, `ListeningPanel.tsx`.

### Опциональные

- визуальные fixtures с длинными списками событий, plan files и workflow nodes.

## Работы

### Обзор

- сохранить Core status, errors, attention, current/history journal;
- сгруппировать важные события и раскрывать payload/details по клику;
- сохранить copy и empty/loading/error states.

### Ревью планов

- сохранить native picker, drag-and-drop, список нескольких Markdown-файлов,
  удаление файла и очистку списка;
- сохранить tier, reviewer count/models, synthesis model, model limits и
  preflight warnings;
- сохранить start/stop/retry, progress roster, history и silent-progress
  warning;
- сохранить result copy/export, revise, stop revision, save revision и
  confirmation replace.

### Память и Pulse

- сохранить memory pending/conflicts/proposals, source filter, select,
  confirm/reject/revise/session-only/supersede;
- сохранить child jobs, leases, dead-letter, Pulse, tool calls и approvals;
- сохранить workspace index status, embeddings, update/rebuild/cancel/status и
  knowledge search;
- сохранить repair phase, tests, diff stat, commit/push/CI/cancel и
  `ready_to_update`.

### Составные задачи

- сохранить templates, input form, run/start/cancel, nodes/dependencies,
  attempts, leases, events, approval, failed/dead-letter states;
- на narrow layout заменить широкий graph на читаемый последовательный список.

### Слух

- сохранить unknown/fail-visible state, enable/pause/resume/stop/refresh;
- сохранить hotkey status, device list/active/default/disconnected/empty;
- сохранить runtime check/download/progress/ready/missing/update/error;
- сохранить runtime `unknown` как отдельное состояние «ещё не проверялось»;
- сохранить episodes, explicit transcript open, proposals accept/reject/mute,
  voice commands accept/reject;
- сохранить quiet hours, blocklists, retention, voice commands/autorun;
- сохранить подтверждения «Забыть последние 5 минут» и «Удалить все
  транскрипты».

## Критерии приёмки

- ни одна текущая кнопка, форма или очередь не исчезла;
- каждая панель имеет normal/loading/empty/error/busy состояния;
- опасные ambient и memory действия сохраняют подтверждение Core;
- long-running review/index/workflow/runtime операции показывают реальную
  фазу и позволяют безопасную отмену там, где она есть;
- панели читаемы при 1024 px без горизонтального scroll всей оболочки.

Для «Слуха» визуально различаются все текущие состояния: `stopped`,
`starting`, `listening`, `paused_by_user`, `paused_by_policy`,
`device_conflict`, `device_disconnected`, `engine_unavailable` и `denied`.

Для repair сохраняются фазы `idle`, `available`, `preparing`, `diagnosing`,
`ready_to_commit`, `committing`, `ready_to_push`, `pushing`, `waiting_ci`,
`ready_to_update`, `failed` и `cancelled`.

## Проверки

- focused tests всех существующих panel actions;
- `npm test -- --run`;
- `npm run typecheck`;
- `npm run build`;
- ручные screenshots normal/loading/empty/error для каждой панели.
