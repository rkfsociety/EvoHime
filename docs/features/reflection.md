# Agent Self-Reflection

## Overview

The self-reflection loop is a quality gate that runs after every tool execution in the ReAct loop (`crates/agent-runtime/src/agent_loop/react.rs`). It analyzes the tool output and decides whether the agent should continue, retry, ask the user, or revise the plan.

## Architecture

```
Tool executed (react loop)
    ↓
Observation (output or tool error)
    ↓
ReflectionStage::execute()
    ├─ load failure_pattern / verification_rule from experience memory (6.21)
    ├─ ReflectionEngine::analyze_tool_output()  → score 0.0–1.0 + error patterns
    ├─ 3 failing steps in a row → RevisePlan
    └─ persist row in reflection_events
    ↓
Emit agent.reflection event (WS + session history)
    ↓
React loop applies the action:
    ├─ hint appended to the tool observation the model reads next
    ├─ RetryTool re-opens the identical call for the duplicate guard
    └─ RevisePlan/Escalate emit the `revising_plan` status phase
```

Reflection is on by default; `EVOHIME_REFLECTION_ENABLED=0` turns the whole stage
off and restores the pre-8.2 loop behaviour.

## Reflection Analysis

### Success Score

The engine computes a success score (0.0–1.0) based on:
- **Explicit errors**: Tool.error is set → score = 0.0
- **Silent failures**: Output is empty or contains a failure marker (`failed`, `error`, `ошиб`, `не найден`, `не удалось`, `traceback` — the tool runtime reports its own failures in Russian) → score *= 0.5
- **Remembered failure pattern matched** → score *= 0.5
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

| Action | Meaning | What the loop actually does |
| --- | --- | --- |
| `Proceed` | Tool succeeded (score ≥ 0.8) | Continue; no hint is added to the observation |
| `AskUser` | Doubtful result (0.3–0.8) | Hint tells the model to verify before building on it, and to ask the user if it stays unclear. It is **not** a blocking approval gate — a real ask-gate is 8.4 |
| `RetryTool` | Likely failure (score < 0.3) | Hint plus the duplicate-call guard is re-opened for that exact call, so one identical retry is allowed within the retry budget |
| `RevisePlan` | 3 failing steps in a row | Hint tells the model to stop retrying the approach; `agent.status` phase `revising_plan` is emitted. Automatic re-planning through 8.1 is not wired yet |
| `Escalate` | Critical error | Same as `RevisePlan`; the engine does not produce this verdict yet |

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
cargo test -p evohime-agent-runtime reflection
```

Covers the engine, the stage, and `failed_tool_observation_is_reflected_and_hinted`
(the react loop emits one reflection per observation and the hint reaches the next
model turn). `reflection_stage_uses_experience_memory_and_persists` needs PostgreSQL
and skips itself when the database is unavailable.

## Frontend

`ReflectionTimeline.tsx` renders a collapsed "Самопроверка" block under the task
trace. Only non-`proceed` verdicts are listed — a run where everything went well
shows nothing.

## Future (8.3+)

- Automatic plan revision driven by reflection history (8.1 + 8.3)
- Meta-cognitive confidence in the ask-gate (8.4)
- Counterfactual dry-run for high-impact tools (8.5)
- Active learning: flag uncertain reflections for user review (8.10)
