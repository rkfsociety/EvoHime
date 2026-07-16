# Extended checkpoints and resume (6.12)

> Дата: 2026-07-16  
> Статус: approved via «делай» after 6.11

## Цель

Checkpoint хранит не только `workspace_context`, а план, причину паузы и approval wait. Resume пропускает завершённые шаги и не гоняет planning заново, если план уже есть.

## Checkpoint `state_json`

```json
{
  "workspace_context": "…",
  "plan": [PlanStep…],
  "pause_reason": "approval_required" | "server_restart" | null,
  "approval_wait": {
    "approval_id": "uuid",
    "tool_name": "…",
    "permission": "…",
    "scope": "…"
  }
}
```

Прогресс шагов и tool outputs — из `task_steps` (уже пишутся). Checkpoint **мержится**, не перезаписывается целиком.

## Resume

`AgentResumeContext`:
- `workspace_context`
- `plan` (optional)
- `completed_step_ids` (из completed `task_steps.input_json.plan_step_id`)
- `tool_results` (из step outputs)
- `pause_reason` / `approval_wait` (для логов/продолжения)

Если есть `plan` → skip planning LLM → execute только незавершённые шаги → respond/replan как обычно.

## Тесты

- merge checkpoint preserves plan when updating workspace_context
- resume context skips completed steps (agent-runtime unit/integration)
