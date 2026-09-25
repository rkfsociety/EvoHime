# План 176.3 — Guided Capability Recipes: IPC, guided UI и fork

## Зависимости

### Блокирующие

- [Preflight, run и recovery](./176-2-guided-capability-recipes.md).
- Existing authenticated IPC, workflow/evidence projections, Electron
  navigation and user-owned workflow/package creation paths.

### Опциональные

- Existing Operations/Workbench panels for contextual evidence display.

## Реализация

- Добавить additive typed IPC только для recipe catalog, preflight, start,
  recipe-link lookup и fork/draft handoff; до изменения inventory-ить все
  command/event tags. Run status и cancellation остаются существующими
  `GetWorkflowRun`/`CancelWorkflow` operations; не вводить их дубли.
- Add guided flow `Goal -> Inputs -> Preflight -> What will happen -> Run ->
  Inspect evidence -> Compare/modify -> Save draft`, keeping all decisions in
  Core and payloads bounded/redacted.
- Render the flow through existing ordinary Workbench/Operations/workflow
  surfaces; do not add an internal Core/agent/model tab or a renderer-owned
  recipe catalog.
- Показывать только подтверждённые binding metadata: expected steps,
  provider/model/context refs when supplied by their owner, tools, approval
  points, evidence refs, verification contract and capability gaps. Не выводить
  предположения как разрешённые capabilities и не показывать secrets, hidden
  reasoning, raw prompt/output.
- Отдельно показывать `ReproduceExact`, `ReRunWithCurrentCompatible` и
  `ForkAndModify`. Пока существующие execution/context owners не сохраняют
  необходимые revision pins, первые два состояния должны быть typed
  `unavailable` с причиной; не подменять их запуском текущих revisions.
- Inspect status через bounded `WorkflowRunProjection`; не подписываться на
  `ListWorkflowEvents`, который включает сохранённый `payload_json`. Recipe
  evidence projection содержит только allowlisted hashes, revisions, ids,
  states и redaction summary.
- Fork successful run в новый user-owned `VisualWorkflowBuilder` draft либо
  через существующий workflow-package path с recipe provenance. Для built-in
  передавать исходное template definition с placeholders, а не instantiated
  run graph, чтобы не переносить пользовательские inputs в draft. Если точная
  template revision недоступна — typed refusal. Не переносить credentials,
  grants, context/artifact allowlists и runtime output; сохранить или усилить
  approval, budget и cancellation policy. Draft нельзя публиковать или
  запускать автоматически.

## Критерии

- Renderer cannot alter definition, requirements, resolved revisions, grants or
  approval result; malformed/unknown IPC major fails closed.
- UI distinguishes blocked, warning, running, completed and failed states and
  exposes evidence/provenance sufficient for reproducibility.
- Catalog entries without a compatible existing workflow binding are shown as
  unsupported with a stable reason code and cannot be started or presented as a
  successful capability result.
- UI exposes only bounded run/evidence projections and never parses generic
  workflow event payloads or displays raw prompt/output.
- Run status and cancellation continue to use the existing workflow IPC
  authority; fork creates a new user-owned draft without inherited secrets or
  grants.
- Fork does not copy instantiated user inputs, workflow node outputs or a stale
  template revision into the new draft.
- Fork preserves existing approval and resource limits while clearing inherited
  grants and data allowlists.
- Existing ordinary navigation remains intact; no internal Core/agent/model tab.

## Non-goals

Direct provider calls, renderer-owned recipe catalog, raw transcript display and
automatic mutating actions.
