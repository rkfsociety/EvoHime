# Subagent fan-out (`agent.run`) Implementation Plan

> **For agentic workers:** Implement task-by-task. Steps use checkbox syntax.

**Goal:** Ship tool `agent.run` with concurrency/depth/step/timeout budgets (roadmap 7.31).

**Architecture:** Registry stub in tool-runtime; real execution in agent-runtime (like `memory.search`). Child = `run_agent_loop` with `is_subagent` + depth; process-wide semaphore.

**Tech Stack:** Rust, existing agent-loop / tool-runtime / env config.

## Global Constraints

- No separate child tasks / no UI panel work
- Default depth 1 (no nested `agent.run`)
- Minimize protocol changes (reuse tool events)

---

### Task 1: Tool stub `agent.run`

**Files:** `crates/tool-runtime/src/tools/agent.rs`, `mod.rs`, `registry.rs`, `lib.rs`

- [ ] Add NAME/DESCRIPTION/PERMISSIONS/TIMEOUT + `parse_input`
- [ ] Register in bootstrap; fix list-order assertions in registry tests
- [ ] Unit test parse_input
- [ ] `cargo test -p evohime-tool-runtime`

### Task 2: Subagent budget + runner

**Files:** `crates/agent-runtime/src/subagent.rs`, `lib.rs`, `AgentConfig` fields

- [ ] `SubagentBudget::from_env()`
- [ ] Global `Semaphore` for max concurrent
- [ ] `run_subagent(...)` wrapping `run_agent_loop` with capped config
- [ ] Unit tests for budget defaults / clamp

### Task 3: Wire execute path

**Files:** `agent_loop/execute.rs`, `mod.rs` (pass gateway), parse/REGISTERED_TOOLS, plan prompts, native_tools

- [ ] Dispatch `agent.run` like `memory.search`
- [ ] Pass `&ModelGateway` into `execute_plan_steps`
- [ ] Skip lifecycle events when `config.is_subagent`
- [ ] Tests: depth reject; tool_input for agent.run

### Task 4: Docs + commit

- [ ] `.env.example`, roadmap, current-state, AGENTS
- [ ] Commit `feat(agent): subagent fan-out via agent.run with budget (7.31)`
