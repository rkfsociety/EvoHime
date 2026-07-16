# Plan Executor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bounded plan→execute→observe→replan loop with dependency batch parallel execution.

**Architecture:** Keep orchestration in `agent-runtime`; reuse `task-engine::dependency_batches`.

**Tech Stack:** Rust, tokio, existing ModelGateway + ToolRegistry.

## Global Constraints

- Max 3 replan rounds after initial execute.
- Parallelism only within a dependency batch.
- No protocol schema change required if PlanStep unchanged.
- Commit after completion; no push.

---

### Task 1: Replan parsing + batch execute refactor

- [x] Add failing tests for replan JSON and batch execution helpers
- [x] Refactor `execute_plan_steps` to use `dependency_batches` + parallel batch run
- [x] Wire observe/replan loop in `run_agent_loop_inner`
- [x] `cargo test -p evohime-agent-runtime`

### Task 2: Docs + roadmap

- [x] Mark `6.11` progress in roadmap/current-state
- [x] Commit
