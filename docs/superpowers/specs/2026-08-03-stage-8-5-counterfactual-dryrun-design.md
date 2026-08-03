# Stage 8.5: Counterfactual Dry-Run for High-Impact Tool Calls — Design

**Status:** Approved for planning
**Roadmap item:** `docs/roadmap.md` § Stage 8.5 — "Counterfactual dry-run для high-impact tool calls перед approval"

## Problem

When the agent requests approval for a tool call (`ToolError::NeedsApproval`), the user currently sees only the tool name, permission, and scope. For `filesystem.patch` they additionally see a unified diff (`ApprovalReview::UnifiedDiff`). For every other tool — including high-impact ones like `filesystem.write`, `git.push`, `shell.execute` — the user approves blind.

## Non-goals (explicit)

- No real sandboxed execution / simulation of `shell.execute`. Shell commands are Turing-complete; a substring-pattern "dry run" would be actively misleading (looks authoritative, isn't). We report `Unavailable` instead of pretending.
- No live git preview (`git log @{u}..HEAD`, `git diff --cached --stat`) for `git.push`/`git.commit` in v1. This requires shelling out from a new call site and is deferred — tracked as a natural v2 extension of the `Unavailable` branch.
- No persistence, audit table, caching layer, or new HTTP endpoint. The prediction is cheap, synchronous, and computed once per approval request — there is nothing to cache or audit that doesn't already happen via the existing `permission_approval_audit` table.
- No new WebSocket event. Reuses the existing `approval.required` event's `review` field.

## Architecture

Extend the existing preview mechanism instead of building a parallel one.

Today: `crates/server/src/task/pipeline.rs` catches `ToolError::NeedsApproval`, calls `approval_review(&tool, &input)` (in `crates/server/src/task/approval_review.rs`), and embeds the `Option<ApprovalReview>` result directly into the `ServerEvent::ApprovalRequired` event before sending it over the WebSocket. This already happens synchronously, in-process, with no DB round-trip — exactly the shape a dry-run preview needs.

Changes:

1. **`crates/tool-runtime`** gains a risk-classification module operating on the *resolved* tool call (name + actual JSON input), not the pre-execution plan. This is distinct from `agent-runtime::agent_loop::risk_engine::RiskLevel` (Stage 8.4), which classifies a whole *plan* before any step has concrete parameters and is used for the ask-gate threshold. The two serve different moments (planning-time vs. approval-time) and different data (steps vs. resolved input); tool-runtime is the correct home because it already owns the tool registry and both `agent-runtime` and `server` already depend on it (not the reverse), so no dependency cycle. `agent-runtime`'s existing risk engine is left untouched — no regression risk to Stage 8.4.

2. **`crates/protocol`** extends `ApprovalReview` with two new variants alongside the existing `UnifiedDiff`, and adds a `risk_level: String` field to `ServerEvent::ApprovalRequired` (string, not a shared typed enum — matches the existing convention `RiskLevel::as_str()` already uses when crossing into a `ServerEvent` in Stage 8.4's `confidence_emit.rs`, and avoids adding a `protocol → tool-runtime` dependency for one field).

3. **`crates/server/src/task/approval_review.rs`** becomes total (always returns a review, never `None`) and gains real, exact prediction for `filesystem.write` by reading current file state through the same `WorkspaceSandbox` the real tool execution uses — same path-resolution and traversal protection, no new sanitization logic to get wrong.

4. **`crates/server/src/task/pipeline.rs`** passes the effective workspace root (worktree-aware) into `approval_review`, and threads `risk_level` into the `ApprovalRequired` event.

5. **Frontend `ApprovalModal`** renders the new variants and the risk badge.

## Data model

### `ToolRiskLevel` (new, `crates/tool-runtime/src/risk.rs`)

```
enum ToolRiskLevel { None, Low, Medium, High }  // Ord: None < Low < Medium < High
fn classify_call_risk(tool_name: &str, input: &serde_json::Value) -> ToolRiskLevel
```

Classification table (verified against each tool's actual input schema, not guessed):

| Risk | Tools | Rationale |
|---|---|---|
| None | `filesystem.read`, `filesystem.search`, `filesystem.list`, `git.status`, `git.diff`, `browser.open`, `browser.extract`, `browser.session.read`, `browser.session.screenshot`, `memory.search`, `http.fetch` | Read-only. `http.fetch` confirmed GET-only in its `Input` struct — no `method` field exists, so it cannot mutate. |
| Low | `git.pull`, `browser.session.navigate`, `worker.run` | Can introduce changes but scoped/reversible; `worker.run` submits to an isolated worker, not the workspace. |
| Medium | `filesystem.write`, `filesystem.patch`, `git.commit`, `mcp.call`, `browser.session.click`, `browser.session.type`, `agent.run` | Mutating but locally recoverable (git history, re-writable files) or delegated (subagent has its own budget/approval gate). `mcp.call`'s `method` field is caller-defined JSON-RPC — genuinely opaque, so it's classified conservatively rather than guessed. |
| High | `shell.execute`, `git.push` | Unbounded (arbitrary shell) or hard to reverse (published history). |

Unknown/unregistered tool names default to `Medium` (fail conservative, not fail silent-safe).

### `ApprovalReview` (extended, `crates/protocol/src/lib.rs`)

```
#[serde(tag = "kind", rename_all = "snake_case")]
enum ApprovalReview {
    UnifiedDiff { path: String, diff: String },                                  // existing — filesystem.patch
    FileWrite { path: String, change: FileChangeKind, old_bytes: Option<u64>, new_bytes: u64 }, // new — filesystem.write
    Unavailable { reason: String },                                              // new — everything else
}

#[serde(rename_all = "snake_case")]
enum FileChangeKind { Create, Overwrite }
```

`FileWrite` reports byte counts, not a computed diff — computing a real diff would require adding a diff-algorithm dependency (`similar`/`diffy`) that the workspace doesn't currently have, for a case (`filesystem.write`, typically used to create new files or fully replace content) that's already secondary to `filesystem.patch` (which the agent uses when a precise diff matters, and which already has exact `UnifiedDiff`). Byte counts + create/overwrite is exact, honest, and requires zero new dependencies.

`Unavailable.reason` is a short human string, e.g. `"shell command execution cannot be safely predicted"`, `"git push preview not yet supported"`, `"remote MCP method effects are opaque"` — tailored per tool so the UI message is specific, not generic boilerplate.

### `ServerEvent::ApprovalRequired` (extended)

Adds one field: `risk_level: String` (values: `"none"`, `"low"`, `"medium"`, `"high"`), populated from `ToolRiskLevel::as_str()`. `review` field type unchanged (`Option<ApprovalReview>`) but is now always `Some(..)` in practice — kept `Option` for backward wire-compatibility rather than a breaking schema change.

## Component responsibilities

- **`classify_call_risk`** (tool-runtime): pure function, no I/O, no async. Table lookup + one opaque-default branch. Fully unit-testable without a workspace or DB.
- **`approval_review`** (server): the only async, I/O-touching piece — reads at most one file (for `filesystem.write`) through the existing sandbox. On read failure (permission error, race where file vanished between check and read), falls back to `Unavailable { reason: "could not read current file state" }` rather than propagating an error that would abort the whole approval flow — a broken preview must never block the approval request itself from reaching the user.
- **Frontend `ApprovalModal`**: pure rendering, three branches (`UnifiedDiff`, `FileWrite`, `Unavailable`) plus a risk badge driven by `risk_level`. No new async state, no loading spinner — the field arrives already populated in the event that opens the modal.

## Error handling

- File-read failure while building `FileWrite` → degrade to `Unavailable`, never fail the approval request.
- Unknown tool name in `classify_call_risk` → `Medium` (conservative default), not a panic or `Option::None`.
- No new fallible network/DB calls are introduced by this feature, so there is no new class of "silently swallowed error" to guard against.

## Testing

- `crates/tool-runtime/src/risk.rs`: unit tests, one per risk tier, plus the unknown-tool-name default case and the `http.fetch` None-not-Medium case (regression guard against the earlier flawed design's "https → Low" heuristic).
- `crates/server/src/task/approval_review.rs`: extend existing unit tests — `filesystem.patch` unchanged, new cases for `filesystem.write` (create vs. overwrite vs. unreadable-existing-file), and one representative `Unavailable` case (e.g. `shell.execute`).
- `crates/protocol/src/lib.rs`: extend existing `ApprovalReview`/`ApprovalRequired` serde round-trip test to cover the two new variants and the new `risk_level` field.
- Frontend: typecheck + build after regenerating protocol types; no new test framework needed, existing `ApprovalModal` test coverage (if any) extended with the two new render branches.

## Rollout

No feature flag. This extends an existing, always-on code path (`approval_review` is already called unconditionally whenever an approval is requested) with strictly additive information — a tool that previously showed nothing now shows either a real prediction or an honest "unavailable," never a regression from current behavior.

## Future extensions (not in this plan)

- `git.push`/`git.commit` live preview via `git log`/`git diff --stat`, once a shared "preview" entry point is exposed from `tools::git` (currently `run_git` is private to that module).
- Computed unified diff for `filesystem.write` (would need a diff-algorithm dependency).
- `mcp.call` per-method risk refinement if MCP servers start advertising method-level side-effect metadata.
