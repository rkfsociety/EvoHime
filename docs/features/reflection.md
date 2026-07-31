# Agent Self-Reflection

## Overview

The self-reflection loop is a quality gate that runs after every tool execution. It analyzes the tool output and decides whether the agent should continue, retry, ask the user, or revise the plan.

## Architecture

```
Tool executed
    ↓
Tool output received
    ↓
ReflectionEngine.analyze_tool_output()
    ↓
Success score 0.0–1.0 + error patterns
    ↓
Determine action (Proceed | AskUser | Retry | RevisePlan | Escalate)
    ↓
Emit agent.reflection event
    ↓
Agent loop responds to action
```

## Reflection Analysis

### Success Score

The engine computes a success score (0.0–1.0) based on:
- **Explicit errors**: Tool.error is set → score = 0.0
- **Silent failures**: Output is empty or contains "failed"/"error" → score *= 0.5
- **Expected outcome mismatch**: Output doesn't contain expected substring → score *= 0.7
- **Tool-specific heuristics**:
  - `filesystem.read`: empty output → score *= 0.3
  - `shell.execute`: "not found" or "No such" → score *= 0.2
  - `git.commit`: "nothing to commit" → score *= 0.5

### Error Pattern Matching

If experience memory contains known failure patterns (from `6.21` learning), the engine matches tool output against pattern names and includes them in the analysis with confidence scores.

### Confidence

- If score ≥ 0.7: confidence = 0.9 (high)
- If score < 0.7: confidence = 0.6 (moderate)

## Reflection Actions

| Action | Meaning | Next Step |
| --- | --- | --- |
| `Proceed` | Tool succeeded (score ≥ 0.8) | Continue agent loop normally |
| `AskUser` | Tool failed but might recover (0.3–0.8) | Emit `approval.required` gate; wait for user |
| `RetryTool` | Likely transient failure (score < 0.3) | Re-execute the same tool with same params |
| `RevisePlan` | Systematic failure; plan is wrong | Trigger 8.1 planner to revise plan |
| `Escalate` | Critical error; abort task | Stop agent loop; report to task runner |

## Protocol Events

### agent.reflection

Emitted after tool execution. Contains analysis, action, and recommendation.

```json
{
  "type": "agent.reflection",
  "task_id": "uuid",
  "timestamp": "2026-07-31T12:00:00Z",
  "reflection_type": "post_tool_execution",
  "tool_call_id": "uuid",
  "analysis": {
    "success_score": 0.5,
    "confidence": 0.6,
    "reasoning": "Output doesn't match expected: file contents.",
    "error_patterns": [
      {
        "pattern_id": "E_EMPTY",
        "pattern_name": "empty_file_read",
        "confidence": 0.7,
        "source": "experience_memory"
      }
    ]
  },
  "action": "ask_user",
  "recommendation": null
}
```

## Database

Reflection events are persisted in `reflection_events` table:

```sql
SELECT * FROM reflection_events
WHERE task_id = 'task-uuid'
ORDER BY timestamp ASC;
```

## Testing

Run reflection tests:

```bash
cargo test reflection --lib
cargo test --test reflection_integration_test
```

## Future (8.3+)

- Reflection history used for plan revision (8.2)
- Counterfactual dry-run for high-impact tools (8.5)
- Active learning: flag uncertain reflections for user review (8.10)
