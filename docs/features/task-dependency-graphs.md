# Task Dependency Graphs (Stage 8.3)

## Overview

Stage 8.3 adds explicit task dependency graphs to the planning system. Tasks with no dependencies can now execute in batches, improving performance and enabling better recovery from partial failures.

## Architecture

```
LLM generates plan with dependencies
    ↓
Graph validation (Kahn O(V+E), cycle detection)
    ↓
Topological sort → execution batches
    ↓
Execute batches sequentially
    (steps within batch ordered deterministically)
    ↓
Track per-step state in DB
    ↓
WebSocket events to frontend (real-time status)
    ↓
DAG visualization (React Flow) with status colors
```

## Key Concepts

### PlanStep.depends_on

```rust
pub struct PlanStep {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub depends_on: Vec<String>, // Step IDs that must complete first
}
```

### Execution Batches

A batch is a set of steps that must execute deterministically (ordered by step_id). Subsequent batches wait for prior batch completion.

Example: `A → [B, C] → D`
- Batch 0: [A]
- Batch 1: [B, C] (independent, but ordered lexicographically)
- Batch 2: [D]

### Database Schema

**task_execution_graphs** (versioned)
- `topological_order`: canonical step execution order
- `dependency_map`: task_id → [dep_ids]
- `version`: increments on plan regeneration

**task_execution_steps** (per-step tracking)
- `status`: pending | running | completed | failed
- `started_at`, `completed_at`: timestamps
- `error_message`: failure reason

### Failure Handling

- One step fails → descendants are skipped (marked failed)
- Siblings continue executing
- Execution halts after 3 cumulative failures

### Backward Compatibility

Empty `depends_on` is materialized to sequential (depends on previous step).

Example: Legacy plan from Stage 8.1:
```
steps: [A, B, C]
depends_on: [[], [], []]
```
After loading:
```
depends_on: [[], [A], [B]]  // Sequential execution preserved
```

## Frontend

**React Flow DAG Viewer**
- Interactive nodes (draggable, zoom, pan)
- Color-coded status: pending (gray) | running (blue+animation) | completed (green) | failed (red)
- Automatic layout by topological depth
- Real-time updates via WebSocket

## Configuration

| Env Var | Default | Purpose |
|---------|---------|---------|
| `EVOHIME_GRAPH_MAX_STEPS` | 50 | Max steps per plan |
| `EVOHIME_GRAPH_MAX_FAILURES` | 3 | Halt after N total failures |

## Critical Fixes (REVISED v4)

### Blocker #1: Backward Compatibility ✅
Empty `depends_on` materialized to sequential dependencies in `build_executable_plan`.

### Blocker #2: Kahn O(V+E) ✅
BTreeSet for queue instead of Vec with remove(0)+sort. O(log V) per operation.

### Blocker #3: Cumulative Failures ✅
`total_failures` counter, never reset, max 3 per execution.

### Blocker #4: Parallelism Clarity ✅
Stage 8.3 = batching + sequential execution. Parallel deferred to Stage 8.4.

## Testing

- Unit tests: cycle detection, missing deps, topological sort, diamond pattern
- Batch computation: independent steps, sequential deps, ready-state checks
- Plan generation: materialized deps, fallback handling, size limits
- Frontend: React Flow component with status rendering

## Implementation

**Modules:**
- `crates/agent-runtime/src/planning_graph.rs` — Kahn algorithm, validation
- `crates/agent-runtime/src/plan_generation.rs` — LLM response handling, backward compat
- `crates/agent-runtime/src/agent_loop/graph_executor.rs` — Batch computation
- `crates/storage/src/planning_graph.rs` — DAO, versioning, transaction safety
- `frontend/web/src/components/TaskDependencyGraph.tsx` — React Flow viewer

**Migrations:**
- `2026-08-03-000001-task-execution-graphs.sql` — Schema + pgcrypto setup

## Next Steps

- Task 4 (ReAct integration): Wire batching into main loop + cumulative failure tracking
- Task 6 (E2E tests): Full pipeline validation
- Stage 8.4: True parallel execution within batches via tokio::join_all
