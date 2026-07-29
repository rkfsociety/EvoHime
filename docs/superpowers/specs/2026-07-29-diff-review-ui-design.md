# Diff Review UI (`7.106`) Design

**Date:** 2026-07-29
**Status:** Approved for implementation planning
**Roadmap item:** `7.106` — Diff review UI for agent patches before apply

## Goal

Show the complete unified diff for an agent-proposed `filesystem.patch`
operation before the operator grants its existing `FilesystemWrite`
approval. The operator can inspect the exact patch and either apply it once
or deny it.

## Scope

This milestone covers only `filesystem.patch` calls whose effective
`FilesystemWrite` permission mode is `Ask`.

It does not:

- add review for `filesystem.write`, shell commands, Git actions, or other
  mutating tools;
- make the diff editable;
- add partial hunk selection;
- replace the existing permission model;
- force a review when the effective permission mode is `Allow`;
- introduce a REST endpoint or database table for review payloads.

`Allow` remains the explicit operator choice to bypass per-operation
approvals. `Deny` continues to reject the tool before it executes.

## Existing Flow

1. The agent runtime prepares a `filesystem.patch` input containing `path`
   and `patch`.
2. The tool registry derives the permission scope from the unparsed input.
3. The permission layer returns `NeedsApproval` for `FilesystemWrite` in
   `Ask` mode.
4. The server stores the pending input in the task checkpoint and emits
   `approval.required`.
5. The browser currently shows only tool name, permission, and scope.
6. Granting the approval resumes the task, which executes the same pending
   input.

The checkpoint already contains the authoritative input. The missing piece
is a typed, bounded presentation of that input in `approval.required`.
Because the current permission check happens before tool execution parses
the input, `7.106` also adds patch-specific preflight validation before the
permission loop.

## Architecture

### Protocol

`ApprovalRequiredEvent` gains an optional `review` field. The only review
variant in `7.106` is:

```json
{
  "kind": "unified_diff",
  "path": "frontend/web/src/app.tsx",
  "diff": "@@ -10,3 +10,4 @@\n old\n+new"
}
```

The protocol schema defines a closed `UnifiedDiffReview` object:

- `kind` is the constant `unified_diff`;
- `path` is the relative workspace path from the validated tool input;
- `diff` is the complete unified diff that will be supplied to
  `filesystem.patch`;
- additional properties are rejected.

`review` is optional on `ApprovalRequiredEvent`. Existing clients and all
non-patch approval events therefore retain their current behavior.

The protocol workflow remains:

1. edit `crates/protocol/schema/evohime.protocol.schema.json`;
2. update `crates/protocol/src/lib.rs`;
3. run `npm run generate:protocol`;
4. keep `frontend/web/src/protocol.ts` as the re-export boundary.

`protocol.generated.ts` must not be edited manually.

### Server-side review construction

The server constructs review data at the point where it handles
`ToolError::NeedsApproval`. A focused helper accepts the tool name and the
pending JSON input and returns an optional protocol review:

```rust
fn approval_review(tool_name: &str, input: &Value) -> Option<ApprovalReview>
```

It returns `Some(UnifiedDiff)` only when:

- `tool_name == "filesystem.patch"`;
- `input.path` is a string;
- `input.patch` is a string;
- the patch passed the normal tool input/schema limits.

Malformed or non-patch input produces `None` and preserves the ordinary
coarse approval modal. Review generation must never panic or prevent the
server from pausing a task safely.

The review is derived from the exact pending input. The frontend does not
parse arbitrary tool JSON and does not infer review eligibility.

### Resource limit

The `patch` field is limited to 131,072 UTF-8 bytes (128 KiB) before an
approval can be created. `crates/tool-runtime/src/tools/patch.rs` exposes a
focused input validator/parser, and `ToolRegistry` invokes it for
`filesystem.patch` before entering the permission loop. Patch execution
reuses the same validation logic rather than defining a second limit. The
limit is also represented in the tool JSON Schema.

The complete accepted patch is sent to the browser. There is no truncation:
the operator must never be asked to approve content that the UI did not
receive. An oversized patch fails with a safe error telling the agent to
split the change into smaller patches.

This limit applies to all `filesystem.patch` executions, including `Allow`
mode, so permission settings cannot bypass the resource boundary.

### Frontend

`ApprovalModal` remains the single approval surface.

When `request.review?.kind === "unified_diff"`, it renders:

- title `Проверка патча`;
- the relative target path;
- a scrollable monospaced diff;
- line styling for additions, removals, and hunk headers;
- buttons `Запретить` and `Применить патч`.

The review is read-only. The UI sends the existing
`approval.granted`/`approval.denied` commands and never returns modified
patch text.

The patch-review variant does not show `Запомнить путь (1 ч)`. A remembered
path would allow later patches to skip the review while the permission mode
still appears to be `Ask`. Operators who intentionally want that behavior
can change `FilesystemWrite` to `Allow` in Settings.

For approvals without a review, the current modal, copy, buttons, and
remember-path behavior remain unchanged.

Diff rendering is extracted into a small presentation component shared with
the Git panel, so line classification and markup are not duplicated. The
component receives a string and renders it; it does not fetch data or decide
whether an operation is safe.

### Accessibility and responsive behavior

The modal keeps the existing focus trap, Escape-to-deny behavior,
`role="dialog"`, `aria-modal`, and labelled title.

The diff region:

- is keyboard-scrollable;
- has an accessible label;
- preserves whitespace;
- uses the existing added/removed/context colors with sufficient contrast;
- fits the viewport with bounded height and horizontal scrolling;
- keeps action targets at least 44 px on mobile.

## Data Flow

```text
agent tool call
  -> registry preflight validates filesystem.patch input and 128 KiB limit
  -> permission check returns NeedsApproval
  -> ToolError carries the exact pending input
  -> server derives ApprovalReview::UnifiedDiff
  -> approval.required(review) is persisted and sent over WebSocket
  -> ApprovalModal renders the full read-only diff
  -> operator grants or denies with existing commands
  -> granted task resumes with the unchanged checkpoint input
  -> filesystem.patch applies that exact input
```

The review payload and executed payload have a single source: the pending
tool input. No client-side reconstruction or edited copy is introduced.

## Failure Handling

- Missing or malformed `path`/`patch`: omit the review and show the ordinary
  approval modal; task remains safely paused.
- Patch larger than 128 KiB: reject before approval and ask the agent to
  split it.
- Unknown future review kind: generated TypeScript exhaustiveness and a
  frontend fallback preserve the ordinary approval details.
- WebSocket reconnect/history replay: `review` is part of the persisted
  `approval.required` event, so the same modal can be reconstructed.
- Grant or deny send failure: retain the current modal, matching existing
  behavior.
- Patch context changes after approval: `filesystem.patch` continues to
  reject the execution with its existing context/removal mismatch errors.

## Security and Privacy

- Review data is sent only through the authenticated session WebSocket and
  follows existing session ownership checks.
- The server exposes only `path` and `patch`; it does not include unrelated
  tool input or environment data.
- The patch is already stored in the task checkpoint. Adding it to the
  persisted approval event does not introduce a new class of data, but it
  makes the review available during replay.
- React renders diff content as text, never as HTML.
- Scope validation and sandbox path resolution remain authoritative at tool
  execution time.

## Testing

### Protocol

- `ApprovalRequiredEvent` round-trips with and without
  `UnifiedDiffReview`.
- Generated TypeScript matches the JSON Schema.
- Protocol drift checks pass.

### Tool runtime

- patch preflight runs before an approval is created;
- a patch at or below 128 KiB proceeds to normal execution;
- a patch above 128 KiB returns a stable invalid-input error;
- malformed inputs retain their current safe errors.

### Server

- `approval_review` returns the exact path and diff for
  `filesystem.patch`;
- it returns `None` for other tools and malformed patch input;
- emitted `approval.required` contains the review while the checkpoint keeps
  the same pending input.

### Frontend

- the shared diff classifier identifies added, removed, hunk, and context
  lines;
- patch review shows `Применить патч` and never shows remember-path;
- ordinary approvals keep their current controls;
- typecheck and production build pass.

### Regression commands

- relevant Rust unit tests for protocol, tool runtime, and server;
- `npm test`, `npm run typecheck`, and `npm run build` in `frontend/web`;
- `npm run generate:protocol` followed by the existing protocol drift check;
- `git diff --check`.

## Documentation and audit trail

Implementation uses separate commits for:

1. protocol and server review payload;
2. patch-size resource boundary;
3. shared diff renderer and approval UI;
4. roadmap/current-state/AGENTS reconciliation.

The implementation plan records exact files, test names, commands, and
commit boundaries. On completion, `7.106` is marked complete everywhere and
`7.107` becomes the only remaining Stage 7 item.
