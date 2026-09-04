# Routes and views

There is no URL router. `App.tsx` owns a `ViewId` state and renders panels in one Electron window.

## Primary user views

- `chat` — `TaskTimeline` plus optional `WorkbenchPanel` and browser panel; default view and the primary user workflow.
- `overview` — `OverviewPanel`, system/workspace summary.
- `reviews` — `PlanReviewPanel`, plan review flow.
- `operations` — `OperationsPanel`, self-repair, memory, Pulse, child tasks, workspace index and refinement.
- `scheduled` — `ScheduledPanel`, local automation schedules with owner, UTC time, revision and enabled/paused state.
- `workflows` — workflow panels and visual/composer tools.
- `continuations` — `ContinuationPanel`.
- `kernels` — `AnalysisKernelPanel`.
- `listening` — `ListeningPanel`.
- `human-work-items` — `HumanWorkItemsPanel`.

## Developer views

The sidebar also exposes quick actions `Новый чат`, `Запланировано` and `Плагины`.
The developer menu contains technical panels for plans, workspaces, protocols,
safety, execution backends, model resilience, roles, artifacts, collaboration
and termination. It is opened from the account menu as `Интерфейс разработчика`.
