# Stage 8.5: Counterfactual Dry-Run for High-Impact Tool Calls — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Before a user approves a high-impact tool call, show them a real, honest prediction of what will happen — exact for `filesystem.write`/`filesystem.patch`, an explicit "unavailable" for everything else — plus a risk badge, without adding any new subsystem (no DB table, no endpoint, no cache).

**Architecture:** Extend the existing synchronous preview mechanism. `crates/server/src/task/pipeline.rs` already calls `approval_review(&tool, &input)` in-process, with no I/O beyond what's already available, and embeds the result directly into the `ServerEvent::ApprovalRequired` WebSocket event that's already sent. This plan adds a risk classifier (`crates/tool-runtime/src/risk.rs`), two new `ApprovalReview` variants (`FileWrite`, `Unavailable`) alongside the existing `UnifiedDiff`, a `risk_level` field on the event, and matching frontend rendering.

**Tech Stack:** Rust (tool-runtime, protocol, server), TypeScript/React (ApprovalModal), JSON Schema → `json-schema-to-typescript` codegen (`npm run generate:protocol`).

## Global Constraints

- No new dependency, DB table, HTTP endpoint, WebSocket event type, or feature flag — this plan is strictly additive to existing code paths (per design doc "Non-goals" / "Rollout").
- `filesystem.write` prediction reports byte counts and create/overwrite, not a computed diff — no diff-algorithm dependency (per design doc "Data model" note).
- Any file-read failure while building a preview degrades to `Unavailable`, never fails the approval request itself (per design doc "Error handling").
- Unknown tool names classify as `ToolRiskLevel::Medium` (conservative default, not silent-safe) (per design doc risk table).
- `http.fetch` classifies as `None` risk — its `Input` struct has no `method` field, it is GET-only, confirmed by reading `crates/tool-runtime/src/tools/http.rs` (per design doc risk table note).
- Never hand-edit `frontend/web/src/protocol.generated.ts` — it is produced by `npm run generate:protocol` (per `AGENTS.md` protocol workflow).

---

## File Structure

**New:**
- `crates/tool-runtime/src/risk.rs` — `ToolRiskLevel` enum + `classify_call_risk()`, pure/sync, no I/O.

**Modified:**
- `crates/tool-runtime/src/lib.rs` — export the new `risk` module.
- `crates/protocol/schema/evohime.protocol.schema.json` — add `FileWriteReview`, `UnavailableReview` schemas; widen `review` to a `oneOf`; add `risk_level` to `ApprovalRequiredEvent`.
- `crates/protocol/src/lib.rs` — add `FileWrite`/`Unavailable` variants to `ApprovalReview`; add `risk_level: String` to `ServerEvent::ApprovalRequired`; extend the round-trip test.
- `crates/server/src/task/approval_review.rs` — make `approval_review()` total (always returns a value), add real `filesystem.write` prediction, add `classify_call_risk` wiring, extend tests.
- `crates/server/src/task/pipeline.rs` — capture the effective workspace root before it's moved into `AgentConfig`; pass it into `approval_review`; pass `risk_level` into the emitted event.
- `frontend/web/src/protocol.generated.ts` — regenerated, not hand-edited.
- `frontend/web/src/protocol.ts` — re-export the two new generated types.
- `frontend/web/src/lib/approval-review.ts` — add `isFileWriteReview`/`isUnavailableReview` type guards alongside the existing `isPatchReview`.
- `frontend/web/src/components/ApprovalModal.tsx` — render the two new review kinds and a risk badge.
- `frontend/web/src/styles.css` (or the approval-specific partial it imports) — styles for the new preview blocks and risk badge.

---

## Task 1: `ToolRiskLevel` classifier in tool-runtime

**Files:**
- Create: `crates/tool-runtime/src/risk.rs`
- Modify: `crates/tool-runtime/src/lib.rs`

**Interfaces:**
- Produces: `pub enum ToolRiskLevel { None, Low, Medium, High }` (derives `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord`), with `impl ToolRiskLevel { pub fn as_str(&self) -> &'static str }` returning `"none"|"low"|"medium"|"high"`, and `pub fn classify_call_risk(tool_name: &str, _input: &serde_json::Value) -> ToolRiskLevel`.

- [ ] **Step 1: Write the failing tests**

Create `crates/tool-runtime/src/risk.rs`:

```rust
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolRiskLevel {
    None,
    Low,
    Medium,
    High,
}

impl ToolRiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolRiskLevel::None => "none",
            ToolRiskLevel::Low => "low",
            ToolRiskLevel::Medium => "medium",
            ToolRiskLevel::High => "high",
        }
    }
}

/// Classifies a resolved tool call (name + actual JSON input) by risk tier.
/// Distinct from `agent-runtime`'s plan-level `RiskLevel`, which scores a
/// whole plan before any step has concrete parameters.
pub fn classify_call_risk(tool_name: &str, _input: &Value) -> ToolRiskLevel {
    match tool_name {
        "filesystem.read" | "filesystem.search" | "filesystem.list" | "git.status"
        | "git.diff" | "browser.open" | "browser.extract" | "browser.session.read"
        | "browser.session.screenshot" | "memory.search" | "http.fetch" => ToolRiskLevel::None,

        "git.pull" | "browser.session.navigate" | "worker.run" => ToolRiskLevel::Low,

        "filesystem.write" | "filesystem.patch" | "git.commit" | "mcp.call"
        | "browser.session.click" | "browser.session.type" | "agent.run" => {
            ToolRiskLevel::Medium
        }

        "shell.execute" | "git.push" => ToolRiskLevel::High,

        _ => ToolRiskLevel::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_read_only_tools_as_none() {
        for tool in [
            "filesystem.read",
            "filesystem.search",
            "filesystem.list",
            "git.status",
            "git.diff",
            "browser.open",
            "browser.extract",
            "browser.session.read",
            "browser.session.screenshot",
            "memory.search",
        ] {
            assert_eq!(
                classify_call_risk(tool, &json!({})),
                ToolRiskLevel::None,
                "{tool} should be None risk"
            );
        }
    }

    #[test]
    fn classifies_http_fetch_as_none_because_it_is_get_only() {
        // http.fetch's Input struct has no `method` field — it can never mutate.
        assert_eq!(
            classify_call_risk("http.fetch", &json!({"url": "https://example.com"})),
            ToolRiskLevel::None
        );
    }

    #[test]
    fn classifies_low_risk_tools() {
        for tool in ["git.pull", "browser.session.navigate", "worker.run"] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::Low);
        }
    }

    #[test]
    fn classifies_medium_risk_tools() {
        for tool in [
            "filesystem.write",
            "filesystem.patch",
            "git.commit",
            "mcp.call",
            "browser.session.click",
            "browser.session.type",
            "agent.run",
        ] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::Medium);
        }
    }

    #[test]
    fn classifies_high_risk_tools() {
        for tool in ["shell.execute", "git.push"] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::High);
        }
    }

    #[test]
    fn unknown_tool_defaults_to_medium_not_none() {
        assert_eq!(
            classify_call_risk("some.future.tool", &json!({})),
            ToolRiskLevel::Medium
        );
    }

    #[test]
    fn risk_levels_are_ordered() {
        assert!(ToolRiskLevel::None < ToolRiskLevel::Low);
        assert!(ToolRiskLevel::Low < ToolRiskLevel::Medium);
        assert!(ToolRiskLevel::Medium < ToolRiskLevel::High);
    }

    #[test]
    fn as_str_matches_expected_wire_values() {
        assert_eq!(ToolRiskLevel::None.as_str(), "none");
        assert_eq!(ToolRiskLevel::Low.as_str(), "low");
        assert_eq!(ToolRiskLevel::Medium.as_str(), "medium");
        assert_eq!(ToolRiskLevel::High.as_str(), "high");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (module not yet wired)**

Run: `cargo test -p evohime-tool-runtime risk::`
Expected: compile error, `risk` module not declared in `lib.rs` yet.

- [ ] **Step 3: Wire the module**

Edit `crates/tool-runtime/src/lib.rs`, add `mod risk;` alongside the other `mod` declarations and export the public items:

```rust
mod cdp;
mod registry;
mod risk;
mod sandbox;
mod shell_env;
mod ssrf;
mod tools;

pub use registry::{ToolContext, ToolError, ToolProgress, ToolRegistry, ToolResult};
pub use risk::{classify_call_risk, ToolRiskLevel};
pub use sandbox::WorkspaceSandbox;
pub use ssrf::{
    allow_private_targets, assert_safe_http_url, effective_host_allowlist, host_allowlist_from_env,
    lock_host_allowlist, lock_private_override, HostAllowlistGuard, PrivateOverrideGuard,
};
pub use tools::agent;
pub use tools::browser;
pub use tools::filesystem;
pub use tools::git;
pub use tools::mcp;
pub use tools::memory;
pub use tools::worker;
pub use tools::{patch, search, shell, write};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p evohime-tool-runtime risk::`
Expected: 8 tests pass (`classifies_read_only_tools_as_none`, `classifies_http_fetch_as_none_because_it_is_get_only`, `classifies_low_risk_tools`, `classifies_medium_risk_tools`, `classifies_high_risk_tools`, `unknown_tool_defaults_to_medium_not_none`, `risk_levels_are_ordered`, `as_str_matches_expected_wire_values`).

- [ ] **Step 5: Commit**

```bash
git add crates/tool-runtime/src/risk.rs crates/tool-runtime/src/lib.rs
git commit -m "feat(tool-runtime): add call-level tool risk classifier"
```

---

## Task 2: Extend `ApprovalReview` protocol (schema + Rust)

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`

**Interfaces:**
- Consumes: nothing from Task 1 (protocol crate does not depend on tool-runtime; `risk_level` crosses as a plain `String`, matching the existing convention already used by Stage 8.4's `confidence_emit.rs`).
- Produces:
  ```rust
  #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum ApprovalReview {
      UnifiedDiff { path: String, diff: String },
      FileWrite { path: String, change: FileChangeKind, old_bytes: Option<u64>, new_bytes: u64 },
      Unavailable { reason: String },
  }

  #[serde(rename_all = "snake_case")]
  pub enum FileChangeKind { Create, Overwrite }
  ```
  and `ServerEvent::ApprovalRequired` gains `risk_level: String`.

- [ ] **Step 1: Update the JSON schema**

Edit `crates/protocol/schema/evohime.protocol.schema.json`. Replace the existing `"UnifiedDiffReview"` definition block (currently at line 319) with itself unchanged plus two new sibling definitions, and widen the `ApprovalRequiredEvent.review` field to a `oneOf` of all three. Also add `risk_level` to `ApprovalRequiredEvent`'s required/properties.

Replace:
```json
    "UnifiedDiffReview": {
      "type": "object",
      "required": ["kind", "path", "diff"],
      "properties": {
        "kind": { "const": "unified_diff" },
        "path": { "type": "string" },
        "diff": { "type": "string" }
      },
      "additionalProperties": false
    },
    "ApprovalRequiredEvent": {
      "type": "object", "required": ["type", "approval_id", "task_id", "tool_name", "permission", "scope", "created_at"],
      "properties": {
        "type": { "const": "approval.required" }, "approval_id": { "$ref": "#/$defs/Uuid" }, "task_id": { "$ref": "#/$defs/Uuid" }, "tool_name": { "type": "string" }, "permission": { "type": "string" }, "scope": { "type": "string" }, "review": { "$ref": "#/$defs/UnifiedDiffReview" }, "created_at": { "$ref": "#/$defs/DateTime" }
      }, "additionalProperties": false
    },
```

With:
```json
    "UnifiedDiffReview": {
      "type": "object",
      "required": ["kind", "path", "diff"],
      "properties": {
        "kind": { "const": "unified_diff" },
        "path": { "type": "string" },
        "diff": { "type": "string" }
      },
      "additionalProperties": false
    },
    "FileWriteReview": {
      "type": "object",
      "required": ["kind", "path", "change", "new_bytes"],
      "properties": {
        "kind": { "const": "file_write" },
        "path": { "type": "string" },
        "change": { "type": "string", "enum": ["create", "overwrite"] },
        "old_bytes": { "type": "integer", "minimum": 0 },
        "new_bytes": { "type": "integer", "minimum": 0 }
      },
      "additionalProperties": false
    },
    "UnavailableReview": {
      "type": "object",
      "required": ["kind", "reason"],
      "properties": {
        "kind": { "const": "unavailable" },
        "reason": { "type": "string" }
      },
      "additionalProperties": false
    },
    "ApprovalRequiredEvent": {
      "type": "object", "required": ["type", "approval_id", "task_id", "tool_name", "permission", "scope", "risk_level", "created_at"],
      "properties": {
        "type": { "const": "approval.required" }, "approval_id": { "$ref": "#/$defs/Uuid" }, "task_id": { "$ref": "#/$defs/Uuid" }, "tool_name": { "type": "string" }, "permission": { "type": "string" }, "scope": { "type": "string" }, "risk_level": { "type": "string", "enum": ["none", "low", "medium", "high"] }, "review": { "oneOf": [{ "$ref": "#/$defs/UnifiedDiffReview" }, { "$ref": "#/$defs/FileWriteReview" }, { "$ref": "#/$defs/UnavailableReview" }] }, "created_at": { "$ref": "#/$defs/DateTime" }
      }, "additionalProperties": false
    },
```

- [ ] **Step 2: Update the Rust enum and event**

Edit `crates/protocol/src/lib.rs`, replace lines 18-22:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalReview {
    UnifiedDiff { path: String, diff: String },
}
```

With:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalReview {
    UnifiedDiff {
        path: String,
        diff: String,
    },
    FileWrite {
        path: String,
        change: FileChangeKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_bytes: Option<u64>,
        new_bytes: u64,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Create,
    Overwrite,
}
```

Then find the `ApprovalRequired` variant of `ServerEvent` (around line 164-174) and add `risk_level`:

```rust
    #[serde(rename = "approval.required")]
    ApprovalRequired {
        approval_id: Uuid,
        task_id: Uuid,
        tool_name: String,
        permission: String,
        scope: String,
        risk_level: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        review: Option<ApprovalReview>,
        created_at: DateTime<Utc>,
    },
```

- [ ] **Step 3: Update the existing round-trip test to add `risk_level` and add new-variant coverage**

In `crates/protocol/src/lib.rs`, find `round_trips_approval_event_and_commands` (around line 424) and replace it:

```rust
    #[test]
    fn round_trips_approval_event_and_commands() {
        let event = ServerEvent::ApprovalRequired {
            approval_id: Uuid::nil(),
            task_id: Uuid::nil(),
            tool_name: "filesystem.patch".into(),
            permission: "filesystem_write".into(),
            scope: "src/lib.rs".into(),
            risk_level: "medium".into(),
            review: Some(ApprovalReview::UnifiedDiff {
                path: "src/lib.rs".into(),
                diff: "@@ -1 +1 @@\n-old\n+new".into(),
            }),
            created_at: Utc::now(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "approval.required");
        assert_eq!(json["risk_level"], "medium");
        assert_eq!(json["review"]["kind"], "unified_diff");
        assert_eq!(json["review"]["path"], "src/lib.rs");
        assert_eq!(json["review"]["diff"], "@@ -1 +1 @@\n-old\n+new");
        let decoded: ServerEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(
            decoded,
            ServerEvent::ApprovalRequired {
                review: Some(ApprovalReview::UnifiedDiff { .. }),
                ..
            }
        ));

        let file_write = ServerEvent::ApprovalRequired {
            approval_id: Uuid::nil(),
            task_id: Uuid::nil(),
            tool_name: "filesystem.write".into(),
            permission: "filesystem_write".into(),
            scope: "src/new.rs".into(),
            risk_level: "medium".into(),
            review: Some(ApprovalReview::FileWrite {
                path: "src/new.rs".into(),
                change: FileChangeKind::Create,
                old_bytes: None,
                new_bytes: 42,
            }),
            created_at: Utc::now(),
        };
        let file_write_json = serde_json::to_value(&file_write).unwrap();
        assert_eq!(file_write_json["review"]["kind"], "file_write");
        assert_eq!(file_write_json["review"]["change"], "create");
        assert_eq!(file_write_json["review"]["new_bytes"], 42);
        assert!(file_write_json["review"].get("old_bytes").is_none());
        let decoded_write: ServerEvent = serde_json::from_value(file_write_json).unwrap();
        assert!(matches!(
            decoded_write,
            ServerEvent::ApprovalRequired {
                review: Some(ApprovalReview::FileWrite {
                    change: FileChangeKind::Create,
                    ..
                }),
                ..
            }
        ));

        let unavailable = ServerEvent::ApprovalRequired {
            approval_id: Uuid::nil(),
            task_id: Uuid::nil(),
            tool_name: "shell.execute".into(),
            permission: "shell_execute".into(),
            scope: "workspace".into(),
            risk_level: "high".into(),
            review: Some(ApprovalReview::Unavailable {
                reason: "shell command execution cannot be safely predicted".into(),
            }),
            created_at: Utc::now(),
        };
        let unavailable_json = serde_json::to_value(&unavailable).unwrap();
        assert_eq!(unavailable_json["review"]["kind"], "unavailable");
        assert_eq!(
            unavailable_json["review"]["reason"],
            "shell command execution cannot be safely predicted"
        );

        let ordinary = ServerEvent::ApprovalRequired {
            approval_id: Uuid::nil(),
            task_id: Uuid::nil(),
            tool_name: "shell.execute".into(),
            permission: "shell_execute".into(),
            scope: "workspace".into(),
            risk_level: "high".into(),
            review: None,
            created_at: Utc::now(),
        };
        let ordinary_json = serde_json::to_value(ordinary).unwrap();
        assert!(ordinary_json.get("review").is_none());
        assert_eq!(ordinary_json["risk_level"], "high");

        for command in [
            ClientCommand::ApprovalGranted {
                approval_id: Uuid::nil(),
                remember_path: false,
            },
            ClientCommand::ApprovalGranted {
                approval_id: Uuid::nil(),
```

**Note for implementer:** the test function continues past this point unchanged (the `ClientCommand` loop that follows was already there) — only the portion shown above (event construction + new assertions) is new/modified. Do not delete the remainder of the original test body.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p evohime-protocol`
Expected: `round_trips_approval_event_and_commands` and all other protocol tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/protocol/schema/evohime.protocol.schema.json crates/protocol/src/lib.rs
git commit -m "feat(protocol): add FileWrite/Unavailable approval review variants and risk_level"
```

---

## Task 3: Real `filesystem.write` prediction + total `approval_review()`

**Files:**
- Modify: `crates/server/src/task/approval_review.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `evohime_tool_runtime::{classify_call_risk, WorkspaceSandbox}` (Task 1), `evohime_protocol::{ApprovalReview, FileChangeKind}` (Task 2).
- Produces:
  ```rust
  pub(crate) struct ApprovalPreview {
      pub risk_level: String,
      pub review: ApprovalReview,
  }
  pub(crate) async fn approval_review(
      tool_name: &str,
      input: &Value,
      workspace_root: &std::path::Path,
  ) -> ApprovalPreview
  ```
  This replaces the old synchronous `fn approval_review(tool_name: &str, input: &Value) -> Option<ApprovalReview>` — the function is now `async`, always returns a value (never `None`), and additionally reports risk.

- [ ] **Step 1: Write the failing tests**

Replace the full contents of `crates/server/src/task/approval_review.rs`:

```rust
use evohime_protocol::{ApprovalReview, FileChangeKind};
use evohime_tool_runtime::{classify_call_risk, WorkspaceSandbox};
use serde_json::Value;
use std::path::Path;

pub(crate) struct ApprovalPreview {
    pub risk_level: String,
    pub review: ApprovalReview,
}

pub(crate) async fn approval_review(
    tool_name: &str,
    input: &Value,
    workspace_root: &Path,
) -> ApprovalPreview {
    let risk_level = classify_call_risk(tool_name, input).as_str().to_string();
    let review = match tool_name {
        "filesystem.patch" => unified_diff_review(input),
        "filesystem.write" => file_write_review(input, workspace_root).await,
        _ => unavailable_review(tool_name),
    };
    ApprovalPreview { risk_level, review }
}

fn unified_diff_review(input: &Value) -> ApprovalReview {
    let path = input.get("path").and_then(Value::as_str);
    let diff = input.get("patch").and_then(Value::as_str);
    match (path, diff) {
        (Some(path), Some(diff)) => ApprovalReview::UnifiedDiff {
            path: path.to_string(),
            diff: diff.to_string(),
        },
        _ => unavailable_review("filesystem.patch"),
    }
}

async fn file_write_review(input: &Value, workspace_root: &Path) -> ApprovalReview {
    let (path, content) = match (
        input.get("path").and_then(Value::as_str),
        input.get("content").and_then(Value::as_str),
    ) {
        (Some(path), Some(content)) => (path, content),
        _ => return unavailable_review("filesystem.write"),
    };

    let sandbox = match WorkspaceSandbox::new(workspace_root) {
        Ok(sandbox) => sandbox,
        Err(_) => {
            return ApprovalReview::Unavailable {
                reason: "could not resolve workspace for preview".into(),
            }
        }
    };

    let new_bytes = content.len() as u64;
    match sandbox.resolve_existing(path) {
        Ok(resolved) => match tokio::fs::metadata(&resolved).await {
            Ok(metadata) => ApprovalReview::FileWrite {
                path: path.to_string(),
                change: FileChangeKind::Overwrite,
                old_bytes: Some(metadata.len()),
                new_bytes,
            },
            Err(_) => ApprovalReview::Unavailable {
                reason: "could not read current file state".into(),
            },
        },
        Err(_) => ApprovalReview::FileWrite {
            path: path.to_string(),
            change: FileChangeKind::Create,
            old_bytes: None,
            new_bytes,
        },
    }
}

fn unavailable_review(tool_name: &str) -> ApprovalReview {
    let reason = match tool_name {
        "shell.execute" => "shell command execution cannot be safely predicted".to_string(),
        "git.push" => "git push preview is not yet supported".to_string(),
        "git.commit" => "git commit preview is not yet supported".to_string(),
        "mcp.call" => "remote MCP method effects are opaque".to_string(),
        other => format!("no preview available for {other}"),
    };
    ApprovalReview::Unavailable { reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_protocol::ApprovalReview;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn builds_unified_diff_review_from_patch_input() {
        let dir = tempdir().unwrap();
        let input = json!({
            "path": "src/lib.rs",
            "patch": "@@ -1 +1 @@\n-old\n+new"
        });

        let preview = approval_review("filesystem.patch", &input, dir.path()).await;
        assert_eq!(preview.risk_level, "medium");
        assert_eq!(
            preview.review,
            ApprovalReview::UnifiedDiff {
                path: "src/lib.rs".into(),
                diff: "@@ -1 +1 @@\n-old\n+new".into(),
            }
        );
    }

    #[tokio::test]
    async fn malformed_patch_input_degrades_to_unavailable() {
        let dir = tempdir().unwrap();
        let preview = approval_review(
            "filesystem.patch",
            &json!({"path": "src/lib.rs"}),
            dir.path(),
        )
        .await;
        assert!(matches!(preview.review, ApprovalReview::Unavailable { .. }));
    }

    #[tokio::test]
    async fn filesystem_write_to_new_path_reports_create() {
        let dir = tempdir().unwrap();
        let input = json!({"path": "new.txt", "content": "hello"});

        let preview = approval_review("filesystem.write", &input, dir.path()).await;
        assert_eq!(preview.risk_level, "medium");
        assert_eq!(
            preview.review,
            ApprovalReview::FileWrite {
                path: "new.txt".into(),
                change: evohime_protocol::FileChangeKind::Create,
                old_bytes: None,
                new_bytes: 5,
            }
        );
    }

    #[tokio::test]
    async fn filesystem_write_to_existing_path_reports_overwrite_with_old_size() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("existing.txt"), "0123456789").unwrap();
        let input = json!({"path": "existing.txt", "content": "short"});

        let preview = approval_review("filesystem.write", &input, dir.path()).await;
        assert_eq!(
            preview.review,
            ApprovalReview::FileWrite {
                path: "existing.txt".into(),
                change: evohime_protocol::FileChangeKind::Overwrite,
                old_bytes: Some(10),
                new_bytes: 5,
            }
        );
    }

    #[tokio::test]
    async fn shell_execute_reports_unavailable_with_specific_reason() {
        let dir = tempdir().unwrap();
        let preview = approval_review("shell.execute", &json!({"command": "ls"}), dir.path()).await;
        assert_eq!(preview.risk_level, "high");
        assert_eq!(
            preview.review,
            ApprovalReview::Unavailable {
                reason: "shell command execution cannot be safely predicted".into(),
            }
        );
    }

    #[tokio::test]
    async fn git_push_reports_high_risk_and_unavailable() {
        let dir = tempdir().unwrap();
        let preview = approval_review("git.push", &json!({}), dir.path()).await;
        assert_eq!(preview.risk_level, "high");
        assert!(matches!(preview.review, ApprovalReview::Unavailable { .. }));
    }
}
```

- [ ] **Step 2: Add `tempfile` as a dev-dependency if not already present**

Check `crates/server/Cargo.toml` for a `[dev-dependencies]` section containing `tempfile`. If absent, add it:

```bash
grep -n "tempfile" C:/github/EvoHime/crates/server/Cargo.toml
```

If no match, add under `[dev-dependencies]` (create the section if it doesn't exist):

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests to verify they pass**

Step 1 replaced the whole file — the old sync `Option`-returning `approval_review` and its tests are gone, replaced atomically by the new async, total version and its own tests together (a full-module rewrite, not an incremental extension, so there is no meaningful intermediate red state to check separately).

Run: `cargo test -p evohime-server approval_review::`
Expected: 6 tests pass (`builds_unified_diff_review_from_patch_input`, `malformed_patch_input_degrades_to_unavailable`, `filesystem_write_to_new_path_reports_create`, `filesystem_write_to_existing_path_reports_overwrite_with_old_size`, `shell_execute_reports_unavailable_with_specific_reason`, `git_push_reports_high_risk_and_unavailable`).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/approval_review.rs crates/server/Cargo.toml
git commit -m "feat(server): real filesystem.write dry-run prediction, total approval_review()"
```

---

## Task 4: Wire the new `approval_review()` into the pipeline

**Files:**
- Modify: `crates/server/src/task/pipeline.rs`

**Interfaces:**
- Consumes: `approval_review(tool_name: &str, input: &Value, workspace_root: &Path) -> ApprovalPreview` (Task 3), `ServerEvent::ApprovalRequired { risk_level: String, .. }` (Task 2).
- Produces: nothing new — this task only updates a call site.

- [ ] **Step 1: Capture the effective workspace root before it is moved**

In `crates/server/src/task/pipeline.rs`, find the `AgentConfig` construction (around line 209, shown below) and add a clone immediately before it:

```rust
    let agent_config = AgentConfig {
```

Change to:

```rust
    let effective_workspace_root = workspace_root.clone();
    let agent_config = AgentConfig {
```

- [ ] **Step 2: Update the `NeedsApproval` handler to use the new async, total `approval_review`**

Find (around line 369-401):

```rust
            Err(AgentError::Tool(ToolError::NeedsApproval {
                tool,
                permission,
                scope,
                approval_id,
                input,
            })) => {
                let review = approval_review(&tool, &input);
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::AgentStatus {
                        task_id: task.id,
                        phase: "waiting_approval".into(),
                    },
                )
                .await?;
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::ApprovalRequired {
                        approval_id,
                        task_id: task.id,
                        tool_name: tool.clone(),
                        permission: permission_name(permission).to_string(),
                        scope: scope.clone(),
                        review,
                        created_at: chrono::Utc::now(),
                    },
                )
                .await?;
```

Replace with:

```rust
            Err(AgentError::Tool(ToolError::NeedsApproval {
                tool,
                permission,
                scope,
                approval_id,
                input,
            })) => {
                let preview = approval_review(&tool, &input, &effective_workspace_root).await;
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::AgentStatus {
                        task_id: task.id,
                        phase: "waiting_approval".into(),
                    },
                )
                .await?;
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::ApprovalRequired {
                        approval_id,
                        task_id: task.id,
                        tool_name: tool.clone(),
                        permission: permission_name(permission).to_string(),
                        scope: scope.clone(),
                        risk_level: preview.risk_level,
                        review: Some(preview.review),
                        created_at: chrono::Utc::now(),
                    },
                )
                .await?;
```

- [ ] **Step 3: Run the full server test suite to check for other `approval_review` call sites or event constructors**

```bash
grep -rn "ApprovalRequired {" C:/github/EvoHime/crates/server/src
```

Expected: only the one call site edited above. If any other construction site is found (e.g. in a test helper), add `risk_level: "none".into()` there for consistency and note it — do not leave any other `ServerEvent::ApprovalRequired` construction missing the new required field, since the code will not compile otherwise.

- [ ] **Step 4: Compile check**

Run: `cargo check -p evohime-server`
Expected: no errors. This will fail loudly with "missing field `risk_level`" at every construction site the grep in Step 3 missed — fix any that appear.

- [ ] **Step 5: Run the pipeline module's existing tests**

Run: `cargo test -p evohime-server pipeline::`
Expected: existing tests pass unchanged (this task doesn't add new pipeline-level tests — behavior is already covered by Task 3's unit tests on `approval_review` itself plus Task 2's protocol round-trip test).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/task/pipeline.rs
git commit -m "feat(server): thread risk-aware approval preview into the task pipeline"
```

---

## Task 5: Regenerate protocol TypeScript types

**Files:**
- Modify (generated, not hand-edited): `frontend/web/src/protocol.generated.ts`
- Modify: `frontend/web/src/protocol.ts`

**Interfaces:**
- Consumes: the schema from Task 2.
- Produces: `FileWriteReview`, `UnavailableReview` TS interfaces; `ApprovalRequiredEvent.review?: UnifiedDiffReview | FileWriteReview | UnavailableReview`; `ApprovalRequiredEvent.risk_level: string`.

- [ ] **Step 1: Regenerate**

```bash
cd C:/github/EvoHime && npm run generate:protocol
```

Expected output: `Wrote <repo>/frontend/web/src/protocol.generated.ts` with no errors.

- [ ] **Step 2: Verify the generated shape**

```bash
grep -n "FileWriteReview\|UnavailableReview\|risk_level" C:/github/EvoHime/frontend/web/src/protocol.generated.ts
```

Expected: `FileWriteReview` and `UnavailableReview` interfaces exist, `ApprovalRequiredEvent.risk_level: string;` exists, and `ApprovalRequiredEvent.review?:` is a union of all three interfaces (`json-schema-to-typescript` compiles a property-level `oneOf` into a TS union type). If the union did not generate correctly, check that the `oneOf` in the schema (Task 2, Step 1) uses `$ref` entries exactly as written — `json-schema-to-typescript` requires each `oneOf` branch to resolve to a named `$defs` schema to produce named union members.

- [ ] **Step 3: Re-export the two new types**

Edit `frontend/web/src/protocol.ts`, in the `export type { ... } from "./protocol.generated"` block, add `FileWriteReview` and `UnavailableReview` next to the existing `UnifiedDiffReview`:

```typescript
  ApprovalRequiredEvent,
  UnifiedDiffReview,
  FileWriteReview,
  UnavailableReview,
```

- [ ] **Step 4: Typecheck**

```bash
cd C:/github/EvoHime/frontend/web && npm run typecheck
```

Expected: no errors (nothing consumes the new types yet, so this just confirms the generated file itself is syntactically valid TS).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/protocol.generated.ts frontend/web/src/protocol.ts
git commit -m "chore(protocol): regenerate TS types for FileWrite/Unavailable approval review"
```

---

## Task 6: Frontend type guards for the new review kinds

**Files:**
- Modify: `frontend/web/src/lib/approval-review.ts`
- Create: `frontend/web/src/lib/approval-review.test.ts` (confirmed not to exist yet — this is a new file)

**Interfaces:**
- Consumes: `FileWriteReview`, `UnavailableReview` (Task 5).
- Produces:
  ```typescript
  export type FileWriteReviewRequest = ApprovalRequiredEvent & { review: FileWriteReview };
  export function isFileWriteReview(request: ApprovalRequiredEvent): request is FileWriteReviewRequest;
  export type UnavailableReviewRequest = ApprovalRequiredEvent & { review: UnavailableReview };
  export function isUnavailableReview(request: ApprovalRequiredEvent): request is UnavailableReviewRequest;
  ```

- [ ] **Step 1: Write the failing tests**

Create `frontend/web/src/lib/approval-review.test.ts` with this content:

```typescript
import { describe, expect, it } from "vitest";
import type { ApprovalRequiredEvent } from "../protocol";
import { isFileWriteReview, isUnavailableReview, isPatchReview } from "./approval-review";

const baseEvent: Omit<ApprovalRequiredEvent, "review"> = {
  type: "approval.required",
  approval_id: "00000000-0000-0000-0000-000000000000",
  task_id: "00000000-0000-0000-0000-000000000000",
  tool_name: "filesystem.write",
  permission: "filesystem_write",
  scope: "new.txt",
  risk_level: "medium",
  created_at: new Date().toISOString(),
};

describe("isFileWriteReview", () => {
  it("returns true for a file_write review", () => {
    const request: ApprovalRequiredEvent = {
      ...baseEvent,
      review: { kind: "file_write", path: "new.txt", change: "create", new_bytes: 5 },
    };
    expect(isFileWriteReview(request)).toBe(true);
  });

  it("returns false for a patch review", () => {
    const request: ApprovalRequiredEvent = {
      ...baseEvent,
      tool_name: "filesystem.patch",
      review: { kind: "unified_diff", path: "a.rs", diff: "@@ -1 +1 @@" },
    };
    expect(isFileWriteReview(request)).toBe(false);
    expect(isPatchReview(request)).toBe(true);
  });
});

describe("isUnavailableReview", () => {
  it("returns true for an unavailable review", () => {
    const request: ApprovalRequiredEvent = {
      ...baseEvent,
      tool_name: "shell.execute",
      review: { kind: "unavailable", reason: "shell command execution cannot be safely predicted" },
    };
    expect(isUnavailableReview(request)).toBe(true);
  });

  it("returns false when review is absent", () => {
    const request: ApprovalRequiredEvent = { ...baseEvent, review: undefined };
    expect(isUnavailableReview(request)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd C:/github/EvoHime/frontend/web && npx vitest run src/lib/approval-review.test.ts
```

Expected: FAIL — `isFileWriteReview`/`isUnavailableReview` not exported yet.

- [ ] **Step 3: Implement**

Replace `frontend/web/src/lib/approval-review.ts` with:

```typescript
import type {
  ApprovalRequiredEvent,
  UnifiedDiffReview,
  FileWriteReview,
  UnavailableReview,
} from "../protocol";
import { isRememberableApprovalScope } from "./approval-scope.ts";

export type PatchReviewRequest = ApprovalRequiredEvent & {
  review: UnifiedDiffReview;
};

export function isPatchReview(request: ApprovalRequiredEvent): request is PatchReviewRequest {
  return request.tool_name === "filesystem.patch" && request.review?.kind === "unified_diff";
}

export type FileWriteReviewRequest = ApprovalRequiredEvent & {
  review: FileWriteReview;
};

export function isFileWriteReview(
  request: ApprovalRequiredEvent,
): request is FileWriteReviewRequest {
  return request.review?.kind === "file_write";
}

export type UnavailableReviewRequest = ApprovalRequiredEvent & {
  review: UnavailableReview;
};

export function isUnavailableReview(
  request: ApprovalRequiredEvent,
): request is UnavailableReviewRequest {
  return request.review?.kind === "unavailable";
}

export function canRememberApprovalPath(request: ApprovalRequiredEvent) {
  return !isPatchReview(request) && isRememberableApprovalScope(request.scope);
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd C:/github/EvoHime/frontend/web && npx vitest run src/lib/approval-review.test.ts
```

Expected: all 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/lib/approval-review.ts frontend/web/src/lib/approval-review.test.ts
git commit -m "feat(web): add type guards for FileWrite/Unavailable approval reviews"
```

---

## Task 7: Render the new review kinds and risk badge in `ApprovalModal`

**Files:**
- Modify: `frontend/web/src/components/ApprovalModal.tsx`
- Modify: `frontend/web/src/styles/memory-responsive.css` — despite the filename, this partial already holds the full base (non-responsive) `.approvalModal`/`.approvalScope`/`.approvalActions` block (lines 103-119), not just responsive overrides. Confirmed by direct inspection; do not go looking elsewhere.

**Interfaces:**
- Consumes: `isFileWriteReview`, `isUnavailableReview` (Task 6), `request.risk_level: string` (Task 5).
- Produces: no new exports — this is a leaf component.

- [ ] **Step 1: Confirm the target file is unchanged before editing**

```bash
grep -n "approvalRememberButton" C:/github/EvoHime/frontend/web/src/styles/memory-responsive.css
```

Expected: one match, `.approvalRememberButton { background: rgba(91, 134, 255, 0.22); color: var(--text-0); }` — this is the last line of the existing approval block; the new rules in Step 4 are appended after it.

- [ ] **Step 2: Update imports and add the risk badge + new review branches**

Edit `frontend/web/src/components/ApprovalModal.tsx`. Replace the import block:

```typescript
import { useEffect, useState } from "react";
import type { ApprovalRequiredEvent } from "../protocol";
import { DiffViewer } from "./DiffViewer";
import { useModalA11y } from "../hooks/useModalA11y";
import { canRememberApprovalPath, isPatchReview } from "../lib/approval-review";
import { ConfidenceAndRisk } from "./ConfidenceAndRisk";
import { ForceApproveModal } from "./ForceApproveModal";
```

With:

```typescript
import { useEffect, useState } from "react";
import type { ApprovalRequiredEvent } from "../protocol";
import { DiffViewer } from "./DiffViewer";
import { useModalA11y } from "../hooks/useModalA11y";
import {
  canRememberApprovalPath,
  isFileWriteReview,
  isPatchReview,
  isUnavailableReview,
} from "../lib/approval-review";
import { ConfidenceAndRisk } from "./ConfidenceAndRisk";
import { ForceApproveModal } from "./ForceApproveModal";

const RISK_LABELS: Record<string, string> = {
  none: "Риск отсутствует",
  low: "Низкий риск",
  medium: "Средний риск",
  high: "Высокий риск",
};

function RiskBadge({ level }: { level: string }) {
  return (
    <span className={`approvalRiskBadge approvalRiskBadge--${level}`}>
      {RISK_LABELS[level] ?? level}
    </span>
  );
}
```

Then, inside the component body, after `const canRememberPath = canRememberApprovalPath(request);` add:

```typescript
  const fileWriteReview = isFileWriteReview(request) ? request : null;
  const unavailableReview = isUnavailableReview(request) ? request : null;
```

Then, in the JSX, change the branch that currently only handles `patchReview` (the `{patchReview ? (...) : (...)}` block) to a three-way branch. Replace:

```tsx
        {patchReview ? (
          <>
            <p className="approvalScope">
              Файл: <code>{request.review.path}</code>
            </p>
            <DiffViewer diff={request.review.diff} emptyText="Пустой патч" />
          </>
        ) : (
          <>
            <p>
              Инструмент: <strong>{request.tool_name}</strong>
            </p>
            <p>
              Разрешение: <strong>{request.permission}</strong>
            </p>
            <p className="approvalScope">
              Область: <code>{request.scope}</code>
            </p>
            {confidenceData ? (
```

With:

```tsx
        {patchReview ? (
          <>
            <p className="approvalScope">
              Файл: <code>{request.review.path}</code>
            </p>
            <DiffViewer diff={request.review.diff} emptyText="Пустой патч" />
          </>
        ) : (
          <>
            <p>
              Инструмент: <strong>{request.tool_name}</strong> <RiskBadge level={request.risk_level} />
            </p>
            <p>
              Разрешение: <strong>{request.permission}</strong>
            </p>
            <p className="approvalScope">
              Область: <code>{request.scope}</code>
            </p>
            {fileWriteReview ? (
              <div className="approvalPreview">
                <p>
                  {fileWriteReview.review.change === "create" ? "Будет создан файл" : "Будет перезаписан файл"}:{" "}
                  <code>{fileWriteReview.review.path}</code>
                </p>
                <p className="approvalPreviewMeta">
                  Новый размер: {fileWriteReview.review.new_bytes} байт
                  {fileWriteReview.review.old_bytes != null
                    ? ` (текущий: ${fileWriteReview.review.old_bytes} байт)`
                    : ""}
                </p>
              </div>
            ) : null}
            {unavailableReview ? (
              <p className="approvalPreviewUnavailable">
                Предпросмотр недоступен: {unavailableReview.review.reason}
              </p>
            ) : null}
            {confidenceData ? (
```

- [ ] **Step 3: Verify the rest of the JSX still closes correctly**

The block continues unchanged after `{confidenceData ? (...) : null}` — no further edits needed in that branch. Run a visual diff check:

```bash
cd C:/github/EvoHime && git diff frontend/web/src/components/ApprovalModal.tsx
```

Confirm the diff shows only the import block, the `RiskBadge`/`RISK_LABELS` additions, the two new `const` lines, and the JSX insertions above — no stray braces.

- [ ] **Step 4: Add styles**

Append to `frontend/web/src/styles/memory-responsive.css` (end of file):

```css
.approvalRiskBadge {
  display: inline-block;
  padding: 0.1rem 0.5rem;
  border-radius: 999px;
  font-size: 0.75rem;
  font-weight: 600;
  margin-left: 0.5rem;
  vertical-align: middle;
}

.approvalRiskBadge--none {
  background: var(--color-success-bg, #dcfce7);
  color: var(--color-success-text, #166534);
}

.approvalRiskBadge--low {
  background: var(--color-info-bg, #dbeafe);
  color: var(--color-info-text, #1e40af);
}

.approvalRiskBadge--medium {
  background: var(--color-warning-bg, #fef3c7);
  color: var(--color-warning-text, #92400e);
}

.approvalRiskBadge--high {
  background: var(--color-danger-bg, #fee2e2);
  color: var(--color-danger-text, #991b1b);
}

.approvalPreview {
  background: var(--surface-secondary, #f8fafc);
  border: 1px solid var(--border, #e2e8f0);
  border-radius: 6px;
  padding: 0.75rem;
  margin: 0.75rem 0;
}

.approvalPreviewMeta {
  font-size: 0.85rem;
  color: var(--text-muted, #64748b);
  margin: 0.25rem 0 0;
}

.approvalPreviewUnavailable {
  font-size: 0.85rem;
  font-style: italic;
  color: var(--text-muted, #64748b);
  margin: 0.75rem 0;
}
```

- [ ] **Step 5: Typecheck and build**

```bash
cd C:/github/EvoHime/frontend/web && npm run typecheck && npm run build
```

Expected: both succeed with no errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/components/ApprovalModal.tsx frontend/web/src/styles/memory-responsive.css
git commit -m "feat(web): show risk badge and dry-run preview in ApprovalModal"
```

---

## Task 8: Manual verification in the running app

**Files:** none (verification only)

**Interfaces:** none

- [ ] **Step 1: Start the dev stack**

```powershell
.\start-dev.ps1
```

- [ ] **Step 2: Trigger a `filesystem.write` approval**

In the chat UI, ask the agent to write a new file inside the workspace (e.g. "create a file named scratch-test.txt with the text hello"). When the approval modal appears, confirm:
- A risk badge reading "Средний риск" appears next to the tool name.
- A preview block reads "Будет создан файл: scratch-test.txt" with "Новый размер: 5 байт".

- [ ] **Step 3: Trigger a `filesystem.write` overwrite**

Ask the agent to write to the same file again with different content. Confirm the modal now reads "Будет перезаписан файл" and shows both the new size and "(текущий: N байт)".

- [ ] **Step 4: Trigger an unavailable-preview tool**

Ask the agent to run a shell command (e.g. "run `dir` in the shell"). Confirm the modal shows the high-risk badge and "Предпросмотр недоступен: shell command execution cannot be safely predicted".

- [ ] **Step 5: Confirm `filesystem.patch` is unchanged**

Ask the agent to make a small code edit that goes through `filesystem.patch`. Confirm the diff viewer still renders exactly as before (no risk badge shown in the patch-review branch — this matches the existing `patchReviewModal` layout, which was intentionally left untouched).

- [ ] **Step 6: Clean up**

Deny or approve the pending approvals used for testing; delete `scratch-test.txt` if it was created inside the real workspace (not a throwaway worktree).

---

## Task 9: Update roadmap and AGENTS.md

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `AGENTS.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Update roadmap.md**

Edit `docs/roadmap.md`, find the Stage 8.5 row (currently `| 8.5 | Counterfactual dry-run для high-impact tool calls перед approval | M | ⬜ | работает поверх permission engine (`crates/permissions/`) |`) and replace it:

```markdown
| 8.5 | Counterfactual dry-run для high-impact tool calls перед approval | M | ✅ | Расширяет существующий синхронный approval-preview (`ApprovalReview`): `crates/tool-runtime/src/risk.rs` классифицирует резолвленный вызов инструмента (`ToolRiskLevel` None/Low/Medium/High); `filesystem.write` получает точный предикт (create/overwrite + размеры) через тот же `WorkspaceSandbox`, что и реальное исполнение; всё прочее — честный `Unavailable{reason}` вместо угадывания; `risk_level` в `ApprovalRequired` событии; без новой БД/эндпоинта/кеша — вычисляется синхронно при формировании approval. См. `docs/superpowers/specs/2026-08-03-stage-8-5-counterfactual-dryrun-design.md` |
```

- [ ] **Step 2: Update AGENTS.md**

Edit `AGENTS.md`, after the Stage 8.4 bullet (line 78) add a new bullet:

```markdown
- **Stage 8.5** ✅ Counterfactual Dry-Run: extends the existing synchronous `ApprovalReview` preview instead of adding a parallel subsystem; `crates/tool-runtime/src/risk.rs` classifies each resolved tool call (`ToolRiskLevel`); `filesystem.write` gets an exact create/overwrite + byte-size prediction via the same `WorkspaceSandbox` real execution uses; every other tool reports an honest `Unavailable{reason}` instead of a guessed prediction; `risk_level` added to the `approval.required` WS event; no new DB table, endpoint, or cache. See `docs/superpowers/specs/2026-08-03-stage-8-5-counterfactual-dryrun-design.md`.
```

The `## Roadmap` status table further down in `AGENTS.md` only has rows through `8.1 Tree-of-Thoughts Bounded Planner` — Stages 8.2, 8.3, and 8.4 were never added as their own rows there either. Do not add an 8.5 row to that table; leave it as-is, consistent with how 8.2-8.4 were tracked only via the `### Incomplete / next` bullet list edited above.

- [ ] **Step 3: Commit**

```bash
git add docs/roadmap.md AGENTS.md
git commit -m "docs: mark Stage 8.5 counterfactual dry-run complete"
```

---

## Task 10: Full verification pass

**Files:** none — running checks across the whole plan's changes.

- [ ] **Step 1: Rust workspace tests**

```bash
cd C:/github/EvoHime && cargo test --workspace
```

Expected: all tests pass, including the new ones from Tasks 1-4.

- [ ] **Step 2: Clippy**

```bash
cd C:/github/EvoHime && cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Frontend typecheck, test, build**

```bash
cd C:/github/EvoHime/frontend/web && npm run typecheck && npx vitest run && npm run build
```

Expected: all succeed.

- [ ] **Step 4: Protocol drift check**

```bash
cd C:/github/EvoHime && npm run generate:protocol && git diff --stat frontend/web/src/protocol.generated.ts
```

Expected: no diff (Task 5 already committed the regenerated file — this just confirms nothing drifted since).

- [ ] **Step 5: Clean build artifacts**

Per `AGENTS.md` coding rule 15, remove the workspace `target/` directory if this was a dedicated verification pass and nothing downstream still needs it:

```bash
cd C:/github/EvoHime && rm -rf target
```

- [ ] **Step 6: Final review**

```bash
git log --oneline -10
```

Expected: 9 commits from this plan (Tasks 1-7, 9), in order, each with a clear message. Task 8 was manual verification (no commit) and Task 10 is this check (no commit).

---

## Success Criteria

- `ToolRiskLevel` classifies all currently-registered tools correctly, with `http.fetch` verified as `None` (not the flawed earlier design's `Low`) and unknown tools defaulting to `Medium`.
- `filesystem.write` approvals show an exact create/overwrite prediction with real byte counts, computed via the same sandbox the real write uses.
- `filesystem.patch` approvals are pixel-for-pixel unchanged from before this plan (still `UnifiedDiff`, still no risk badge in that branch, per Task 8 Step 5).
- Every other tool (`shell.execute`, `git.push`, `git.commit`, `mcp.call`, etc.) shows an honest, tool-specific `Unavailable` reason — never a fabricated prediction.
- No new database table, HTTP endpoint, cache, feature flag, or WebSocket event type was introduced.
- `cargo test --workspace`, `cargo clippy -- -D warnings`, `npm run typecheck`, `npx vitest run`, and `npm run build` all pass.
