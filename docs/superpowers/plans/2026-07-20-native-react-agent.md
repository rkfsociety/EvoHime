# Native ReAct Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the planner-first agent loop with a bounded native tool-calling ReAct loop that selects the next action from tool observations, survives approvals/restarts, and exposes only safe execution status to the browser.

**Architecture:** Extend the model gateway message model so assistant tool calls and tool observations round-trip through OpenAI-compatible providers. Add a focused `react` controller under `crates/agent-runtime/src/agent_loop/` that owns iteration, limits, duplicate-call detection, permission-aware execution, checkpoint snapshots, and final reply handling. Keep memory retrieval, `memory.search`, existing permissions, task lifecycle, cancellation, and post-task memory feedback intact.

**Tech Stack:** Rust, Tokio, Axum, SQLx/PostgreSQL, serde/serde_json, OpenAI-compatible Chat Completions, React/TypeScript/Vite, generated protocol schema.

## Global Constraints

- Do not create a new git branch; all work stays on the current branch.
- Do not expose or persist chain-of-thought; only safe phase/tool status, tool names, redacted arguments, outputs, and final text may reach the UI.
- Native ReAct is the only orchestration path after this change; do not retain `collect_plan_steps` or bounded `replan` as a hidden fallback.
- Existing permission, approval, cancellation, memory admission/feedback, and task lifecycle behavior remains enforced.
- Tool outputs and model history must remain bounded before every model request.
- `frontend/web/src/protocol.generated.ts` is generated; update the schema/Rust protocol first and run `npm run generate:protocol`.
- Every completed implementation task ends with focused tests and a commit.

---

### Task 1: Add provider message types for native ReAct history

**Files:**
- Modify: `crates/model-gateway/src/providers/mod.rs:43-63`
- Modify: `crates/model-gateway/src/tools.rs:38-72`
- Modify: `crates/model-gateway/src/providers/literouter.rs:220-300` (the `ApiMessage` and completion conversion area)
- Modify: `crates/model-gateway/src/providers/openai_compatible.rs` (shared request serialization if it duplicates LiteRouter)
- Modify: `crates/model-gateway/src/providers/mock.rs`
- Test: `crates/model-gateway/tests/gateway_stream.rs` or a new `crates/model-gateway/tests/native_react_messages.rs`

**Interfaces:**
- `ChatMessage` must represent `system`, `user`, assistant text, assistant `tool_calls`, and a tool observation with `tool_call_id`.
- `NativeToolCall` remains the canonical `{ id, name, arguments }` value.
- The provider API continues to return `ChatResult { content, tool_calls, usage }`.

- [ ] **Step 1: Write failing serialization tests** for an assistant tool-call message and a tool observation. Assert that the generated OpenAI payload contains `tool_calls` with function name/arguments and `tool_call_id` on the observation.
- [ ] **Step 2: Run the focused gateway tests** with `cargo test -p evohime-model-gateway native_react_messages`. Expected: FAIL because `ChatMessage` cannot carry tool-call metadata.
- [ ] **Step 3: Extend the message model** with explicit serializable variants/metadata rather than embedding protocol JSON in arbitrary `content`. Preserve existing constructors and serialization for system/user/assistant text.
- [ ] **Step 4: Update provider request mapping** so both LiteRouter and OpenAI-compatible providers emit valid Chat Completions messages. Keep tool-call arguments as raw JSON strings and preserve multiple calls in their original order.
- [ ] **Step 5: Update the mock provider** to return a queued sequence of `ChatResult` values, allowing each ReAct iteration to receive a different tool-call or final reply.
- [ ] **Step 6: Run the focused tests** again. Expected: PASS, including existing `cargo test -p evohime-model-gateway` tests.
- [ ] **Step 7: Commit** with `git add crates/model-gateway && git commit -m "feat: support native tool observations"`.

### Task 2: Implement the bounded ReAct controller

**Files:**
- Create: `crates/agent-runtime/src/agent_loop/react.rs`
- Modify: `crates/agent-runtime/src/agent_loop/mod.rs`
- Modify: `crates/agent-runtime/src/agent_loop/context.rs`
- Modify: `crates/agent-runtime/src/agent_loop/util.rs`
- Modify: `crates/agent-runtime/src/native_tools.rs`
- Test: `crates/agent-runtime/tests/agent_loop.rs`
- Test: `crates/agent-runtime/tests/react_loop.rs`

**Interfaces:**
- Add `ReActLimits { max_iterations, max_tool_calls, max_retries_per_call, max_history_chars, timeout }` with environment-backed defaults.
- Add `ReActState` containing `iteration`, `tool_calls`, `history`, `used_memory_ids`, `last_call_fingerprint`, and `pending_approval` data needed by the server checkpoint.
- Add `run_react_loop(config, gateway, tools, messages, event_tx, resume)` returning `AgentRunResult` or `AgentError`.
- `run_agent_loop`, `run_agent_loop_resumed`, and `run_agent_loop_as_subagent` delegate to the ReAct controller and retain their public signatures.

- [ ] **Step 1: Write unit tests** for: immediate `assistant.reply`; tool call followed by observation and reply; multiple calls in one response; max iteration; max total calls; duplicate fingerprint; retryable versus non-retryable error.
- [ ] **Step 2: Run `cargo test -p evohime-agent-runtime --test react_loop`**. Expected: FAIL because the controller and state types do not exist.
- [ ] **Step 3: Extract context assembly** from the old planning path into a single initial message builder that adds workspace rules, project index context, retrieved memory, user history, and the user request. Do not mention a plan or replan in the system prompt.
- [ ] **Step 4: Build the tool catalog** with `openai_tools_for_registry(tools)` plus `assistant.reply`; preserve `memory.search`, `agent.run`, permission metadata, and model compatibility checks.
- [ ] **Step 5: Implement one ReAct iteration**: emit selecting status, call `chat_with_tools_for_route`, append the assistant response to history, return a final `assistant.reply`, or validate each native tool call before execution.
- [ ] **Step 6: Implement observations**: execute each accepted call through `ToolRegistry::execute`, emit existing tool events, append a tool message keyed by the exact `tool_call_id`, and return structured error/retryability information to the model when execution fails.
- [ ] **Step 7: Add bounded execution**: enforce iteration/call/history/timeout limits, reject unknown tools and invalid JSON arguments without invoking runtime, stop repeated identical calls, and never retry non-retryable permission denials.
- [ ] **Step 8: Add cancellation checks** before every model request and tool batch and propagate the existing cancellation token through the controller.
- [ ] **Step 9: Run focused tests** and then `cargo test -p evohime-agent-runtime`. Expected: PASS with no planner/replan path used.
- [ ] **Step 10: Commit** with `git add crates/agent-runtime && git commit -m "feat: add bounded native ReAct loop"`.

### Task 3: Replace planner execution in the server pipeline

**Files:**
- Modify: `crates/server/src/task/pipeline.rs:1-520`
- Modify: `crates/server/src/task/helpers.rs` if agent error mapping needs new variants
- Modify: `crates/server/src/metrics_api.rs` or the existing metrics module if ReAct iteration/tool-call counters need recording
- Test: `crates/agent-runtime/tests/pipeline_integration.rs`
- Test: `crates/server/src/task/pipeline.rs` test module or the existing server integration test location

**Interfaces:**
- The pipeline invokes the ReAct-backed `run_agent_loop*` APIs without `AgentPlanUpdated` persistence or plan-approval gating.
- Tool-level `NeedsApproval` still exits the agent task into the existing approval pause flow.
- Task success/failure still calls `apply_task_memory_feedback` and `persist_structured_memory` exactly once.

- [ ] **Step 1: Add a regression test** proving a task can execute `filesystem.read`, receive its observation, then choose `filesystem.search` and finish with a reply, without emitting or persisting a static plan.
- [ ] **Step 2: Run the focused pipeline test**. Expected: FAIL while the pipeline still depends on planner step persistence.
- [ ] **Step 3: Remove planner approval handling** from the event loop and make `AgentPlanUpdated` no longer required for task execution. Keep legacy task-step finalization only where existing UI/history needs it, or replace it with ReAct call records.
- [ ] **Step 4: Update event handling** so every tool call is persisted with a stable call id, tool name, arguments metadata, output, success, and iteration rather than matching only the first pending step by tool name.
- [ ] **Step 5: Preserve approval behavior**: on `NeedsApproval`, save the pending call id/name/arguments and current ReAct state, pause the task, emit `approval.required`, and resume without re-executing completed observations.
- [ ] **Step 6: Preserve task cancellation and memory feedback** on every exit path, including limits, provider failures, tool failures, approval pause, cancellation, and successful reply.
- [ ] **Step 7: Run `cargo test -p evohime-server` and the focused integration tests**. Expected: PASS with no `plan_approval_required` path for ordinary tasks.
- [ ] **Step 8: Commit** with `git add crates/server crates/agent-runtime/tests && git commit -m "feat: run tasks through ReAct controller"`.

### Task 4: Persist ReAct checkpoints and implement safe resume

**Files:**
- Modify: `crates/agent-runtime/src/agent_loop/mod.rs` or the public resume types
- Modify: `crates/server/src/task/steps.rs:12-60, 90-150`
- Modify: `crates/server/src/task/pipeline.rs:140-150, 280-430`
- Modify: `crates/storage/src/lib.rs:687-740`
- Modify: `crates/storage/src/models.rs` if a typed checkpoint representation is used
- Test: `crates/server/src/task/steps.rs` tests
- Test: `crates/task-engine/tests/lifecycle_integration.rs`

**Interfaces:**
- Checkpoint JSON contains `react_state`, `react_messages`, `completed_call_ids`, `pending_call`, `used_memory_ids`, `iteration`, `tool_call_count`, `pause_reason`, and `approval_wait`.
- `build_agent_resume_context` reconstructs a ReAct resume object and does not synthesize a `PlanStep` from old task rows.
- A completed call with an observation is never executed again during resume; an unresolved pending call is permission-checked again.

- [ ] **Step 1: Add failing checkpoint tests** for serialization/deserialization of a two-iteration history, a pending approval, and a completed call that must not repeat after resume.
- [ ] **Step 2: Run the focused checkpoint tests**. Expected: FAIL because resume currently only understands `plan`, step ids, and text tool results.
- [ ] **Step 3: Define a versioned checkpoint state** with serde structs and a `schema_version` field. Reject malformed state by returning a controlled resume error and mark the task failed/paused according to existing lifecycle behavior.
- [ ] **Step 4: Save a checkpoint after each assistant decision, after each observation, and before approval pause. Bound serialized history and redact tool arguments/outputs according to existing persistence policy.
- [ ] **Step 5: Update resume construction** to restore messages, counters, call fingerprints, used memory ids, and pending approval. Retain compatibility for any old checkpoint that still contains a planner plan.
- [ ] **Step 6: Add lifecycle integration coverage** for approval → resume, server restart → resume, cancellation, and completed observation deduplication.
- [ ] **Step 7: Run `cargo test -p evohime-task-engine -p evohime-server`**. Expected: PASS.
- [ ] **Step 8: Commit** with `git add crates/agent-runtime crates/server crates/storage crates/task-engine && git commit -m "feat: checkpoint ReAct conversations"`.

### Task 5: Add safe ReAct phase protocol status

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Modify: `frontend/web/src/hooks/useServerEventHandler.ts`
- Modify: `frontend/web/src/app.tsx` or the existing task/chat state owner
- Modify: `frontend/web/src/protocol.ts`
- Generated: `frontend/web/src/protocol.generated.ts`
- Test: `crates/protocol/src/lib.rs` tests
- Test: `frontend/web/src` existing test configuration or a focused handler test

**Interfaces:**
- Add a safe `agent.status` server event with `task_id` and a closed phase union: `selecting_action`, `executing_tools`, `waiting_approval`, `responding`, `stopped`.
- No field may contain rationale, raw prompts, hidden model messages, or chain-of-thought.

- [ ] **Step 1: Add schema/Rust serialization tests** for every phase and for rejection of unknown phase values if the existing schema policy supports it.
- [ ] **Step 2: Run protocol tests**. Expected: FAIL before the event is defined.
- [ ] **Step 3: Add the event to the JSON schema and Rust enum**, then run `npm run generate:protocol` from the repository root. Never hand-edit the generated TypeScript file.
- [ ] **Step 4: Update the frontend event handler** to map phase status into the existing task/chat state and show compact status copy while a task is running.
- [ ] **Step 5: Ensure tool outputs and arguments use existing redaction/truncation rules** and that no model message history is rendered in the chat timeline.
- [ ] **Step 6: Run protocol tests and the frontend typecheck/build** with `cargo test -p evohime-protocol` and `cd frontend/web; npm run build`. Expected: PASS.
- [ ] **Step 7: Commit** with `git add crates/protocol frontend/web && git commit -m "feat: expose safe ReAct status"`.

### Task 6: Expand deterministic mock coverage and memory integration tests

**Files:**
- Modify: `crates/model-gateway/src/providers/mock.rs`
- Modify: `crates/agent-runtime/tests/react_loop.rs`
- Modify: `crates/agent-runtime/tests/pipeline_integration.rs`
- Modify: `crates/server/src/task/memory.rs` tests if used-memory attribution needs a regression
- Test: `crates/memory` existing retrieval tests where ReAct `memory.search` is involved

**Interfaces:**
- The mock provider accepts a sequence such as `tool_call(read)`, `tool_call(search)`, `tool_call(reply)` and returns one item per native completion request.
- ReAct tests can inspect emitted events and final result without a network provider or PostgreSQL unless the case specifically tests memory persistence.

- [ ] **Step 1: Add mock scenarios** for sequential calls, parallel calls, invalid tool calls, retryable errors, repeated calls, max limits, and final reply.
- [ ] **Step 2: Add tests** asserting the exact tool observation is included in the next provider request and that the final response is emitted only after `assistant.reply`.
- [ ] **Step 3: Add memory tests** asserting retrieved memory is present as bounded untrusted context, `memory.search` can be called during the loop, and used memory ids still reach helpful/harmful feedback.
- [ ] **Step 4: Run `cargo test --workspace`**. Expected: PASS.
- [ ] **Step 5: Commit** with `git add crates/model-gateway crates/agent-runtime crates/memory crates/server && git commit -m "test: cover ReAct observations and limits"`.

### Task 7: Remove obsolete planner-only paths and update documentation

**Files:**
- Modify: `crates/agent-runtime/src/agent_loop/plan.rs` (remove or retain only generic formatting helpers that are still used)
- Modify: `crates/agent-runtime/src/native_tools.rs` (rename planner prompt/helpers to ReAct tool catalog terminology)
- Modify: `crates/agent-runtime/src/agent_loop/mod.rs` (remove old plan/replan imports and dead code)
- Modify: `docs/current-state.md`
- Modify: `docs/development-plan.md`
- Modify: `AGENTS.md` if the runtime description still says plan → batches → bounded replan
- Test: `cargo test --workspace`

- [ ] **Step 1: Search for stale planner claims** with `rg -n "collect_plan_steps|bounded replan|plan approval|plan → batches|AgentPlanUpdated" crates docs AGENTS.md frontend`.
- [ ] **Step 2: Remove dead planner/replan code** only after all ReAct callers and tests pass. Keep compatibility code only where old persisted checkpoints require it.
- [ ] **Step 3: Update canonical docs** to describe `ReAct: tool call → observation → next action`, bounded limits, safe status-only UI, and the retained memory/RAG flow.
- [ ] **Step 4: Run formatting/lints/builds:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, and `cd frontend/web; npm run build`.
- [ ] **Step 5: Commit** with `git add crates docs AGENTS.md && git commit -m "docs: document native ReAct orchestration"`.

### Task 8: End-to-end verification and live smoke test

**Files:**
- No source changes expected; modify only if verification exposes a concrete regression.
- Inspect: `start-dev.ps1`, `docs/current-state.md`, generated protocol output, and git status.

- [ ] **Step 1: Verify the complete clean tree** with `git status --short --branch` and confirm all intended commits are on the current branch.
- [ ] **Step 2: Start the application using the required launcher** `.\start-dev.ps1`, not separate `cargo run` and `npm run dev` processes.
- [ ] **Step 3: Run a browser smoke scenario** requiring at least two dependent actions: confirm the UI shows selecting/executing status, each tool output, a final reply, and no rationale text.
- [ ] **Step 4: Run an approval scenario** for a protected tool, approve it, and confirm the pending native call executes exactly once.
- [ ] **Step 5: Stop/restart the server or resume a paused task** and confirm the ReAct history continues from the checkpoint without repeating a completed tool call.
- [ ] **Step 6: Inspect server logs/metrics** for bounded iteration/tool-call counts and absence of unhandled provider or serialization errors.
- [ ] **Step 7: Run the final verification suite** from Task 7 and record the exact pass output before claiming completion.

## Final Review Checklist

- [ ] No planner-first execution path remains for normal tasks.
- [ ] Every native tool call has a matching observation keyed by call id.
- [ ] Tool permissions and approvals still gate protected operations.
- [ ] Unknown tools, invalid arguments, repeated calls, timeouts, and token budgets terminate safely.
- [ ] Checkpoints restore conversation state and do not duplicate completed calls.
- [ ] Memory retrieval, `memory.search`, extraction, attribution, feedback, and decay remain functional.
- [ ] UI shows safe phase/tool statuses but no chain-of-thought.
- [ ] Protocol generated files were regenerated from schema.
- [ ] Rust and frontend verification passed.
