# Design: Subagent fan-out via `agent.run` (7.31)

**Date:** 2026-07-17  
**Status:** Approved by repeated “давай следующий пункт” → approach A  
**Scope:** First slice only

## Problem

EvoHime can run independent plan steps in parallel (tool batches), but cannot spawn **child agent loops** with their own prompt, step budget, and isolation from recursive fan-out. Roadmap 7.31 asks for multi-agent / subagent fan-out with a budget.

## Decision

Add tool **`agent.run`**, executed by agent-runtime (same pattern as `memory.search`):

| Field | Type | Notes |
| --- | --- | --- |
| `prompt` | string | required — child user message |
| `max_steps` | u64? | clamp to global max |
| `timeout_ms` | u64? | wall clock for child loop |
| `model_route` | string? | optional; default = parent route |

Output structured: `{ summary, steps_run, truncated, depth }`.

### Budget (env)

| Env | Default | Meaning |
| --- | --- | --- |
| `EVOHIME_SUBAGENT_MAX_CONCURRENT` | `2` | process-wide semaphore |
| `EVOHIME_SUBAGENT_MAX_DEPTH` | `1` | root=0; depth≥max rejects nested `agent.run` |
| `EVOHIME_SUBAGENT_MAX_STEPS` | `6` | hard cap on child plan steps executed |
| `EVOHIME_SUBAGENT_TIMEOUT_MS` | `120000` | default child wall timeout |

### Behavior

1. Parent plan may include multiple `agent.run` without `depends_on` → existing batch `join_all` fans out; semaphore caps concurrency.
2. Child runs `run_agent_loop` with `is_subagent=true`: no `TaskStarted`/`TaskCompleted` lifecycle spam; tool events still use parent `task_id`.
3. Child tools: same registry **except** `agent.run` always rejected when `subagent_depth >= MAX_DEPTH` (default: no nested spawn).
4. Parent cancel: not wired in v1 beyond timeout (follow-up with shared `CancellationToken`); timeout aborts child.
5. Permissions: inherit parent `ToolRegistry` / permission engine.

### Non-goals (this slice)

- Separate child `task_id` / Tasks panel rows
- Git worktrees (`7.107`)
- $ spend caps (`7.108`)
- Frontend-specific subagent UI

## Files

- `crates/tool-runtime/src/tools/agent.rs` — schema stub
- `crates/agent-runtime/src/subagent.rs` — budget + runner
- `crates/agent-runtime/src/agent_loop/execute.rs` — dispatch `agent.run`
- planning / `REGISTERED_TOOLS` / native tools / `.env.example` / roadmap

## Success criteria

- Unit tests: budget parse, depth reject, input parse
- `cargo test -p evohime-tool-runtime -p evohime-agent-runtime`
- Roadmap 7.31 marked ✅ with evidence note
