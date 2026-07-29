# Diff Review UI (`7.106`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the complete, bounded unified diff for a pending `filesystem.patch` approval and let the operator apply or deny that exact patch.

**Architecture:** Add patch-specific preflight validation before the permission loop, extend `approval.required` with an optional typed unified-diff review, derive that review server-side from the authoritative pending tool input, and render it through a shared read-only diff component. Existing approval commands and all non-patch approvals remain unchanged.

**Tech Stack:** Rust 2021, Serde, JSON Schema, Axum/WebSocket protocol, React 18, TypeScript 5.8, Node test runner, CSS.

## Global Constraints

- Work directly in the current `main`; do not create a branch or worktree.
- Cover only `filesystem.patch` when effective `FilesystemWrite` permission is `Ask`.
- Preserve `Allow` as the explicit way to bypass per-operation review.
- Do not add review for `filesystem.write`, shell, Git, or other tools.
- The review is read-only and submits only existing `approval.granted` or `approval.denied` commands.
- Never show a truncated patch for approval.
- Runtime patch limit is exactly 131,072 UTF-8 bytes; Rust validation is authoritative.
- JSON Schema uses `maxLength: 131072` as a model/client bound; multibyte text can hit the stricter Rust byte bound sooner.
- Patch review never offers the one-hour remember-path action.
- Ordinary approvals retain current copy, buttons, and remember-path behavior.
- Do not add frontend business logic; the server decides whether review data exists.
- Do not add npm dependencies.
- Never edit `frontend/web/src/protocol.generated.ts` manually.
- Update `.github/workflows/rust.yml` only if workspace/dependency/lint/test expectations change; this plan does not require such a change.
- After Rust verification, remove workspace `target/` when no subsequent verification needs it.
- Commit each completed task separately; do not push without a direct request.

---

### Task 1: Validate patch input before permission approval

**Files:**
- Modify: `crates/tool-runtime/src/tools/patch.rs`
- Modify: `crates/tool-runtime/src/registry.rs`
- Modify: `crates/tool-runtime/schemas/filesystem.patch.json`

**Interfaces:**
- Produces: `pub const MAX_PATCH_BYTES: usize = 131_072`
- Produces: `pub(crate) fn validate_input(value: &Value) -> Result<(), ToolError>`
- Consumes later: server review construction relies on `NeedsApproval.input` having passed this preflight.

- [ ] **Step 1: Add failing unit tests for the byte limit**

In `crates/tool-runtime/src/tools/patch.rs`, add a `#[cfg(test)]` module with:

```rust
#[test]
fn accepts_patch_at_byte_limit() {
    let value = json!({"path": "src/lib.rs", "patch": "a".repeat(MAX_PATCH_BYTES)});
    assert!(validate_input(&value).is_ok());
}

#[test]
fn rejects_patch_above_byte_limit() {
    let value = json!({"path": "src/lib.rs", "patch": "a".repeat(MAX_PATCH_BYTES + 1)});
    let error = validate_input(&value).expect_err("oversized patch must fail");
    assert!(matches!(
        error,
        ToolError::InvalidInput { message, .. }
            if message == "patch exceeds 131072-byte limit; split it into smaller patches"
    ));
}

#[test]
fn counts_utf8_bytes_not_characters() {
    let value = json!({"path": "src/lib.rs", "patch": "я".repeat((MAX_PATCH_BYTES / 2) + 1)});
    assert!(validate_input(&value).is_err());
}
```

- [ ] **Step 2: Add a failing registry test proving validation precedes approval**

In `crates/tool-runtime/src/registry.rs`, create an Ask-mode `PermissionEngine`,
execute `filesystem.patch` with an oversized patch, and assert the result is
`ToolError::InvalidInput`, not `ToolError::NeedsApproval`:

```rust
#[tokio::test]
async fn oversized_patch_is_rejected_before_approval() {
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(Permission::FilesystemWrite, PermissionMode::Ask)
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "old\n").unwrap();
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: Some(Uuid::new_v4()),
        progress_tx: None,
    };

    let error = registry
        .execute(
            &context,
            "filesystem.patch",
            json!({"path": "file.txt", "patch": "a".repeat(tools::patch::MAX_PATCH_BYTES + 1)}),
        )
        .await
        .expect_err("preflight must reject oversized input");

    assert!(matches!(error, ToolError::InvalidInput { .. }));
}
```

- [ ] **Step 3: Run the focused tests and verify RED**

Run:

```powershell
cargo test -p evohime-tool-runtime patch::tests::
cargo test -p evohime-tool-runtime oversized_patch_is_rejected_before_approval
```

Expected: compilation fails because `MAX_PATCH_BYTES` and `validate_input`
do not exist.

- [ ] **Step 4: Add the shared parser/validator**

In `patch.rs`:

```rust
pub const MAX_PATCH_BYTES: usize = 131_072;

#[derive(Deserialize)]
struct Input {
    path: String,
    patch: String,
}

fn parse_input(value: Value) -> Result<Input, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|error| {
        ToolError::InvalidInput {
            tool: NAME.into(),
            message: error.to_string(),
        }
    })?;
    validate_patch_bytes(&input.patch)?;
    Ok(input)
}

fn validate_patch_bytes(patch: &str) -> Result<(), ToolError> {
    if patch.len() > MAX_PATCH_BYTES {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: "patch exceeds 131072-byte limit; split it into smaller patches".into(),
        });
    }
    Ok(())
}

pub(crate) fn validate_input(value: &Value) -> Result<(), ToolError> {
    let input: Input = serde_json::from_value(value.clone()).map_err(|error| {
        ToolError::InvalidInput {
            tool: NAME.into(),
            message: error.to_string(),
        }
    })?;
    validate_patch_bytes(&input.patch)
}
```

Change `execute` to call `parse_input(value)?` instead of its current inline
`serde_json::from_value`.

- [ ] **Step 5: Run patch preflight before the permission loop**

In `ToolRegistry::execute_with_cancellation`, after resolving the
`ToolDefinition` and before iterating `definition.permissions`, add:

```rust
if name == tools::patch::NAME {
    tools::patch::validate_input(&input)?;
}
```

This ordering is the invariant tested by
`oversized_patch_is_rejected_before_approval`.

- [ ] **Step 6: Add the JSON Schema hint**

Change `crates/tool-runtime/schemas/filesystem.patch.json` so `patch` is:

```json
"patch": {
  "type": "string",
  "maxLength": 131072
}
```

Keep `required` and `additionalProperties: false` unchanged.

- [ ] **Step 7: Run GREEN verification**

Run:

```powershell
cargo test -p evohime-tool-runtime patch::tests::
cargo test -p evohime-tool-runtime oversized_patch_is_rejected_before_approval
cargo test -p evohime-tool-runtime
```

Expected: all tool-runtime tests pass.

- [ ] **Step 8: Commit**

```powershell
git add crates/tool-runtime/src/tools/patch.rs crates/tool-runtime/src/registry.rs crates/tool-runtime/schemas/filesystem.patch.json
git commit -m "feat(tools): bound patch input before approval"
```

---

### Task 2: Add a typed approval review to the shared protocol

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Modify: `crates/server/src/task/pipeline.rs` (set `review: None` until Task 3)
- Regenerate: `frontend/web/src/protocol.generated.ts`
- Modify only if export is absent after generation: `frontend/web/src/protocol.ts`

**Interfaces:**
- Produces Rust:

```rust
pub enum ApprovalReview {
    UnifiedDiff { path: String, diff: String },
}
```

- Produces TypeScript: `UnifiedDiffReview`
- Changes: `ApprovalRequiredEvent.review?: UnifiedDiffReview`
- Consumes later: server builds `ApprovalReview`; frontend renders
  `request.review`.

- [ ] **Step 1: Extend the existing protocol round-trip test first**

Update `round_trips_approval_event_and_commands` in
`crates/protocol/src/lib.rs` to construct:

```rust
let event = ServerEvent::ApprovalRequired {
    approval_id: Uuid::nil(),
    task_id: Uuid::nil(),
    tool_name: "filesystem.patch".into(),
    permission: "filesystem_write".into(),
    scope: "src/lib.rs".into(),
    review: Some(ApprovalReview::UnifiedDiff {
        path: "src/lib.rs".into(),
        diff: "@@ -1 +1 @@\n-old\n+new".into(),
    }),
    created_at: Utc::now(),
};
let json = serde_json::to_value(&event).unwrap();
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
```

Add a second ordinary approval with `review: None` and assert serialized JSON
omits `review`.

- [ ] **Step 2: Run the protocol test and verify RED**

Run:

```powershell
cargo test -p evohime-protocol round_trips_approval_event_and_commands
```

Expected: compilation fails because `ApprovalReview` and the `review` field
do not exist.

- [ ] **Step 3: Add the Rust protocol type**

Before `ServerEvent`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalReview {
    UnifiedDiff { path: String, diff: String },
}
```

Add to `ServerEvent::ApprovalRequired`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
review: Option<ApprovalReview>,
```

Update every existing Rust construction of `ApprovalRequired` to set
`review: None` until Task 3 supplies the patch review.

- [ ] **Step 4: Add the JSON Schema definition**

Add:

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
}
```

Add optional `review` to `ApprovalRequiredEvent.properties`:

```json
"review": { "$ref": "#/$defs/UnifiedDiffReview" }
```

Do not add `review` to the event's required array.

- [ ] **Step 5: Regenerate TypeScript**

Run from repository root:

```powershell
npm run generate:protocol
```

Inspect `frontend/web/src/protocol.generated.ts` and confirm it contains
`UnifiedDiffReview` and `ApprovalRequiredEvent.review?`.

If `frontend/web/src/protocol.ts` does not re-export the generated review
type, add `UnifiedDiffReview` to its type export list.

- [ ] **Step 6: Run GREEN and drift verification**

Run:

```powershell
cargo test -p evohime-protocol
npm run generate:protocol
$firstHash = (Get-FileHash -Algorithm SHA256 frontend/web/src/protocol.generated.ts).Hash
npm run generate:protocol
$secondHash = (Get-FileHash -Algorithm SHA256 frontend/web/src/protocol.generated.ts).Hash
if ($firstHash -ne $secondHash) { throw "protocol generation is not deterministic" }
```

Expected: Rust tests pass and two consecutive protocol generations produce
identical bytes. CI will additionally compare the committed generated file
against a fresh generation.

- [ ] **Step 7: Commit**

```powershell
git add crates/protocol/schema/evohime.protocol.schema.json crates/protocol/src/lib.rs crates/server/src/task/pipeline.rs frontend/web/src/protocol.generated.ts frontend/web/src/protocol.ts
git commit -m "feat(protocol): describe approval diff reviews"
```

If `protocol.ts` did not change, omit it from `git add`.

---

### Task 3: Attach the exact pending patch to `approval.required`

**Files:**
- Create: `crates/server/src/task/approval_review.rs`
- Modify: `crates/server/src/task/mod.rs`
- Modify: `crates/server/src/task/pipeline.rs`

**Interfaces:**
- Consumes: `evohime_protocol::ApprovalReview`
- Produces:

```rust
pub(crate) fn approval_review(tool_name: &str, input: &Value) -> Option<ApprovalReview>
```

- Pipeline passes the returned value directly into
  `ServerEvent::ApprovalRequired.review`.

- [ ] **Step 1: Write focused helper tests**

Create `approval_review.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_unified_diff_review_from_patch_input() {
        let input = json!({
            "path": "src/lib.rs",
            "patch": "@@ -1 +1 @@\n-old\n+new"
        });
        assert_eq!(
            approval_review("filesystem.patch", &input),
            Some(ApprovalReview::UnifiedDiff {
                path: "src/lib.rs".into(),
                diff: "@@ -1 +1 @@\n-old\n+new".into(),
            })
        );
    }

    #[test]
    fn ignores_non_patch_tools_and_malformed_input() {
        assert_eq!(
            approval_review(
                "filesystem.write",
                &json!({"path": "src/lib.rs", "content": "new"})
            ),
            None
        );
        assert_eq!(
            approval_review("filesystem.patch", &json!({"path": "src/lib.rs"})),
            None
        );
        assert_eq!(
            approval_review("filesystem.patch", &json!({"patch": "@@ -1 +1 @@"})),
            None
        );
    }
}
```

- [ ] **Step 2: Run the helper tests and verify RED**

After adding `pub mod approval_review;` in `task/mod.rs`, run:

```powershell
cargo test -p evohime-server approval_review::tests::
```

Expected: compilation fails because the helper is not implemented.

- [ ] **Step 3: Implement the pure helper**

Add:

```rust
use evohime_protocol::ApprovalReview;
use serde_json::Value;

pub(crate) fn approval_review(tool_name: &str, input: &Value) -> Option<ApprovalReview> {
    if tool_name != "filesystem.patch" {
        return None;
    }
    let path = input.get("path")?.as_str()?;
    let diff = input.get("patch")?.as_str()?;
    Some(ApprovalReview::UnifiedDiff {
        path: path.to_string(),
        diff: diff.to_string(),
    })
}
```

Re-export it from `task/mod.rs` with the existing task helper pattern.

- [ ] **Step 4: Wire the helper into the emitted event**

In the `ToolError::NeedsApproval` branch in `pipeline.rs`, derive the review
before moving `tool` or `input` into checkpoint JSON:

```rust
let review = approval_review(&tool, &input);
```

Then emit:

```rust
ServerEvent::ApprovalRequired {
    approval_id,
    task_id: task.id,
    tool_name: tool.clone(),
    permission: permission_name(permission).to_string(),
    scope: scope.clone(),
    review,
    created_at: chrono::Utc::now(),
}
```

Do not alter `approval_wait.input` or `react_pending_call.arguments`; they
remain the authoritative resume payload.

- [ ] **Step 5: Run GREEN verification**

Run:

```powershell
cargo test -p evohime-server approval_review::tests::
cargo test -p evohime-server task::steps::tests::
cargo check -p evohime-server
```

Expected: helper tests pass and the server compiles with the new protocol
field.

- [ ] **Step 6: Commit**

```powershell
git add crates/server/src/task/approval_review.rs crates/server/src/task/mod.rs crates/server/src/task/pipeline.rs
git commit -m "feat(server): include patch review in approvals"
```

---

### Task 4: Extract a shared diff renderer

**Files:**
- Create: `frontend/web/src/lib/diff.ts`
- Create: `frontend/web/src/lib/diff.test.mjs`
- Create: `frontend/web/src/components/DiffViewer.tsx`
- Modify: `frontend/web/src/panels/GitPanel.tsx`
- Modify: `frontend/web/src/styles/panels.css`
- Modify: `frontend/web/package.json`

**Interfaces:**
- Produces:

```ts
export type DiffLineKind = "added" | "removed" | "hunk" | "plain";
export function classifyDiffLine(line: string): DiffLineKind;
```

- Produces:

```tsx
<DiffViewer diff={string} ariaLabel={string} className?: string />
```

- Consumes later: `ApprovalModal` uses `DiffViewer`.

- [ ] **Step 1: Write the failing classifier tests**

Create `diff.test.mjs`:

```js
import assert from "node:assert/strict";
import test from "node:test";
import { classifyDiffLine } from "./diff.ts";

test("classifies unified diff lines without treating file headers as edits", () => {
  assert.equal(classifyDiffLine("+added"), "added");
  assert.equal(classifyDiffLine("+++ b/src/lib.rs"), "plain");
  assert.equal(classifyDiffLine("-removed"), "removed");
  assert.equal(classifyDiffLine("--- a/src/lib.rs"), "plain");
  assert.equal(classifyDiffLine("@@ -1 +1 @@"), "hunk");
  assert.equal(classifyDiffLine(" context"), "plain");
});
```

Change the frontend test script to run every library test:

```json
"test": "node --test --experimental-strip-types src/lib/*.test.mjs"
```

- [ ] **Step 2: Run the frontend tests and verify RED**

Run from `frontend/web`:

```powershell
npm test
```

Expected: the new test fails because `diff.ts` does not exist.

- [ ] **Step 3: Implement classification**

Create `diff.ts`:

```ts
export type DiffLineKind = "added" | "removed" | "hunk" | "plain";

export function classifyDiffLine(line: string): DiffLineKind {
  if (line.startsWith("+") && !line.startsWith("+++")) return "added";
  if (line.startsWith("-") && !line.startsWith("---")) return "removed";
  if (line.startsWith("@@")) return "hunk";
  return "plain";
}
```

- [ ] **Step 4: Implement the presentation-only component**

Create `DiffViewer.tsx`:

```tsx
import { classifyDiffLine } from "../lib/diff";

export function DiffViewer({
  diff,
  ariaLabel,
  className = "",
}: {
  diff: string;
  ariaLabel: string;
  className?: string;
}) {
  return (
    <pre
      className={`diffViewer ${className}`.trim()}
      aria-label={ariaLabel}
      tabIndex={0}
    >
      {(diff || "Нет изменений").split("\n").map((line, index) => {
        const kind = classifyDiffLine(line);
        const lineClass =
          kind === "added"
            ? "diffAdded"
            : kind === "removed"
              ? "diffRemoved"
              : kind === "hunk"
                ? "diffContext"
                : "";
        return (
          <span className={lineClass} key={`${index}-${line}`}>
            {line || " "}
          </span>
        );
      })}
    </pre>
  );
}
```

- [ ] **Step 5: Refactor `GitPanel` to use `DiffViewer`**

Import `DiffViewer` and replace the inline split/map block with:

```tsx
<DiffViewer
  diff={gitDiff}
  ariaLabel={`Изменения${gitDiffPath ? ` для ${gitDiffPath}` : ""}`}
/>
```

Rename `.gitDiffViewer` selectors in `panels.css` to `.diffViewer` without
changing their current colors or spacing.

- [ ] **Step 6: Run GREEN verification**

Run from `frontend/web`:

```powershell
npm test
npm run typecheck
npm run build
```

Expected: tests, typecheck, and build pass; Git panel behavior is unchanged.

- [ ] **Step 7: Commit**

```powershell
git add frontend/web/src/lib/diff.ts frontend/web/src/lib/diff.test.mjs frontend/web/src/components/DiffViewer.tsx frontend/web/src/panels/GitPanel.tsx frontend/web/src/styles/panels.css frontend/web/package.json
git commit -m "refactor(web): share unified diff rendering"
```

---

### Task 5: Render patch review in the approval modal

**Files:**
- Create: `frontend/web/src/lib/approval-review.ts`
- Create: `frontend/web/src/lib/approval-review.test.mjs`
- Modify: `frontend/web/src/components/ApprovalModal.tsx`
- Modify: `frontend/web/src/styles/memory-responsive.css`
- Modify: `frontend/web/src/styles/mobile-shell.css`

**Interfaces:**
- Consumes: `ApprovalRequiredEvent.review?: UnifiedDiffReview`
- Consumes: `DiffViewer`
- Produces presentation helpers:

```ts
export function isPatchReview(request: ApprovalRequiredEvent): boolean;
export function canRememberApprovalPath(request: ApprovalRequiredEvent): boolean;
```

- Existing `onGrant(false)` and `onDeny()` callbacks remain unchanged.

- [ ] **Step 1: Write failing presentation-rule tests**

Create `approval-review.test.mjs`:

```js
import assert from "node:assert/strict";
import test from "node:test";
import {
  canRememberApprovalPath,
  isPatchReview,
} from "./approval-review.ts";

const base = {
  type: "approval.required",
  approval_id: "00000000-0000-0000-0000-000000000000",
  task_id: "00000000-0000-0000-0000-000000000000",
  tool_name: "filesystem.patch",
  permission: "filesystem_write",
  scope: "src/lib.rs",
  created_at: "2026-07-29T00:00:00Z",
};

test("patch review is apply-once and cannot remember the path", () => {
  const request = {
    ...base,
    review: {
      kind: "unified_diff",
      path: "src/lib.rs",
      diff: "@@ -1 +1 @@\n-old\n+new",
    },
  };
  assert.equal(isPatchReview(request), true);
  assert.equal(canRememberApprovalPath(request), false);
});

test("ordinary path approval keeps remember-path behavior", () => {
  assert.equal(isPatchReview(base), false);
  assert.equal(canRememberApprovalPath(base), true);
});
```

The ordinary assertion uses the existing `isRememberableApprovalScope`
inside the implementation, so also add a non-path scope case that expects
`false`.

- [ ] **Step 2: Run tests and verify RED**

Run from `frontend/web`:

```powershell
npm test
```

Expected: failure because `approval-review.ts` does not exist.

- [ ] **Step 3: Implement presentation helpers**

Create:

```ts
import type { ApprovalRequiredEvent } from "../protocol";
import { isRememberableApprovalScope } from "./approval-scope";

export function isPatchReview(request: ApprovalRequiredEvent): boolean {
  return request.review?.kind === "unified_diff";
}

export function canRememberApprovalPath(request: ApprovalRequiredEvent): boolean {
  return !isPatchReview(request) && isRememberableApprovalScope(request.scope);
}
```

These helpers decide presentation only; review eligibility remains
server-authored.

- [ ] **Step 4: Add the patch-review modal variant**

In `ApprovalModal.tsx`:

- import `DiffViewer`, `isPatchReview`, and
  `canRememberApprovalPath`;
- compute `const patchReview = isPatchReview(request)`;
- set the title to `Проверка патча` for patch review and
  `Требуется разрешение` otherwise;
- for patch review, render:

```tsx
<p className="approvalScope">
  Файл: <code>{request.review!.path}</code>
</p>
<DiffViewer
  diff={request.review!.diff}
  ariaLabel={`Предлагаемый патч для ${request.review!.path}`}
  className="approvalDiffViewer"
/>
```

- keep the existing tool/permission/scope details for ordinary approvals;
- set the grant label and aria-label to `Применить патч` for patch review;
- call `onGrant(false)` from that button;
- never render `approvalRememberButton` for patch review;
- preserve Escape-to-deny and all ordinary approval behavior.

Avoid non-null assertions in final code by narrowing
`request.review?.kind === "unified_diff"` into a local `review` variable.

- [ ] **Step 5: Add bounded responsive styles**

In `memory-responsive.css`:

```css
.approvalModal.patchReviewModal {
  width: min(920px, calc(100vw - 32px));
}

.approvalDiffViewer {
  max-height: min(58vh, 620px);
  overflow: auto;
  margin: 14px 0 0;
  border: 1px solid var(--border-0);
  border-radius: 12px;
  background: #070c18;
  white-space: pre;
}
```

Add `patchReviewModal` conditionally to the dialog class. In
`mobile-shell.css` under `max-width: 768px`, ensure:

```css
.approvalActions button {
  min-height: 44px;
}

.approvalDiffViewer {
  max-height: 48vh;
}
```

- [ ] **Step 6: Run frontend verification**

Run from `frontend/web`:

```powershell
npm test
npm run typecheck
npm run build
```

Expected: all pass. Inspect the built UI through the normal
`.\start-dev.ps1` stack only if a live manual check is needed; do not replace
the requested app launcher with standalone server/frontend commands.

- [ ] **Step 7: Commit**

```powershell
git add frontend/web/src/lib/approval-review.ts frontend/web/src/lib/approval-review.test.mjs frontend/web/src/components/ApprovalModal.tsx frontend/web/src/styles/memory-responsive.css frontend/web/src/styles/mobile-shell.css
git commit -m "feat(web): review agent patches before approval"
```

---

### Task 6: Reconcile Stage 7 documentation and run the full gate

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/architecture.md`
- Modify: `docs/current-state.md`
- Modify: `docs/development-plan.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/security/threat-model.md`
- Modify: `docs/superpowers/specs/2026-07-29-diff-review-ui-design.md`
- Modify: `docs/superpowers/plans/2026-07-29-diff-review-ui.md`

**Interfaces:**
- Marks `7.106` complete.
- Makes `7.107` the only unfinished Stage 7 roadmap item.
- Records exact implementation commits and verification commands.

- [ ] **Step 1: Run the complete relevant verification before status claims**

From repository root:

```powershell
cargo test -p evohime-protocol
cargo test -p evohime-tool-runtime
cargo test -p evohime-server approval_review::tests::
cargo check -p evohime-server
npm run generate:protocol
```

From `frontend/web`:

```powershell
npm test
npm run typecheck
npm run build
```

Back at repository root:

```powershell
git diff --check
git status --short
```

Expected: all commands succeed; only files named by this plan are modified.

- [ ] **Step 2: Remove Rust build artifacts**

After all Rust checks are complete and no process uses them, resolve and
verify that the path is exactly `<repo>\target`, then remove it. If the
execution environment blocks deletion, report the policy block explicitly
instead of claiming cleanup.

- [ ] **Step 3: Update canonical status documents**

Apply these consistent facts everywhere:

- `7.105` and `7.106` are complete;
- only `7.107` remains in Stage 7;
- the next task is `7.107 Worktree-aware multi-checkout agent`;
- `7.106` uses typed optional `approval.required.review`, a 128 KiB
  preflight limit, a shared diff renderer, and apply-once patch approval;
- retain the separate Stage 8 deferral of `7.57`–`7.59`.

In `docs/roadmap.md`, mark row `7.106` as `✅` and add the actual commit
hashes from Tasks 1–5. Update dates to 2026-07-29 where the status paragraph
is explicitly dated.

Set both spec and plan status to `Implemented`, and check completed plan
steps only after their commands actually passed.

- [ ] **Step 4: Scan for stale status**

Run:

```powershell
rg -n '7\.106.*⬜|`7\.106`–`7\.107` remain|остаются `7\.106`|незакрыты только `7\.106`' -g '*.md' .
```

Expected: no stale status matches. Then verify positive status references:

```powershell
rg -n '7\.106|7\.107' AGENTS.md docs/architecture.md docs/current-state.md docs/development-plan.md docs/roadmap.md docs/security/threat-model.md
```

- [ ] **Step 5: Commit documentation**

```powershell
git add AGENTS.md docs/architecture.md docs/current-state.md docs/development-plan.md docs/roadmap.md docs/security/threat-model.md docs/superpowers/specs/2026-07-29-diff-review-ui-design.md docs/superpowers/plans/2026-07-29-diff-review-ui.md
git commit -m "docs: mark patch review complete"
```

- [ ] **Step 6: Final repository verification**

Run:

```powershell
git status --short --branch
git log --oneline --decorate -8
```

Expected: `main` is clean except for the known unreadable
`workers/python/.pytest_cache/` warning, and local commits are not pushed.

## Plan self-review

- Spec coverage: protocol, preflight resource bound, exact server-authored
  review, unchanged checkpoint input, frontend read-only review, no
  remember-path, ordinary approval compatibility, accessibility, responsive
  layout, replay, testing, and documentation are each assigned to a task.
- Scope: only `filesystem.patch` in `Ask` mode; no partial hunk editing,
  `filesystem.write` review, new REST endpoint, migration, or dependency.
- Type consistency: Rust `ApprovalReview::UnifiedDiff { path, diff }`
  generates TypeScript `UnifiedDiffReview`; frontend consumes
  `request.review?.kind === "unified_diff"`.
- Size consistency: `MAX_PATCH_BYTES` is 131,072 bytes in Rust; JSON Schema
  uses `maxLength: 131072` as a non-authoritative character-count hint.
- Source of truth: `NeedsApproval.input` supplies both persisted checkpoint
  input and review; the browser never returns edited diff content.
- Commit boundaries: each task is independently reviewable and testable.
- Completeness scan: the plan contains no placeholder markers, deferred
  implementation, or unspecified error-handling steps.
