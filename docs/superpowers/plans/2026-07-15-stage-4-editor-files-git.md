# Stage 4 Editor, Files, and Git Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the browser workspace flow for files, Monaco editing, Git status/diff, and Git mutations.

**Architecture:** Extend the existing Axum workspace module and React workspace panels. Keep filesystem and Git operations behind the existing workspace sandbox and `ToolRegistry`; use the existing session bus for `file.changed` and `git.diff.changed` synchronization.

**Tech Stack:** Rust 2021, Axum 0.7, Tokio, serde, existing tool-runtime, React 18, TypeScript, Vite, Monaco.

## Global Constraints

- Browser only; no Electron, desktop, or mobile client.
- No business logic in frontend; UI renders API responses and server events.
- All workspace paths remain inside `WORKSPACE_ROOT`.
- Never edit `frontend/web/src/protocol.generated.ts` by hand.
- Git write operations remain permission-controlled and tool-backed.
- Add tests for core logic and tools.

---

### Task 1: Harden Workspace File API

**Files:**
- Modify: `crates/server/src/workspace.rs`
- Modify: `crates/server/src/main.rs`
- Create: `crates/server/tests/workspace_api.rs`

**Interfaces:**
- Preserve `GET /api/files?path=<relative-dir>` and `GET/PUT /api/files/content?path=<relative-file>`.
- Add safe `POST /api/files/content?path=<relative-file>&session_id=<uuid>` for creating files through the same save handler.
- Return `400` for traversal/absolute/symlink escape and `404` for missing workspace entries.

- [ ] Write tests for traversal rejection, `.git` filtering, directory listing, read, and save event behavior.
- [ ] Run `cargo test -p evohime-server --test workspace_api` and observe the expected failures for missing hardening/route behavior.
- [ ] Implement one shared relative-path resolver, hidden `.git` filtering, and correct API error mapping.
- [ ] Run the focused server test and then `cargo test -p evohime-server`.
- [ ] Commit `feat: harden workspace file api`.

### Task 2: Add Git Mutation HTTP API and Events

**Files:**
- Modify: `crates/server/src/workspace.rs`
- Modify: `crates/server/src/main.rs`
- Create: `crates/server/tests/git_api.rs`

**Interfaces:**
- Add `POST /api/git/commit` with `{message}` and `session_id` query.
- Add `POST /api/git/pull` and `POST /api/git/push` with optional `{remote, branch}` and `session_id` query.
- Return `{output, structured}` for successful tool-backed operations and publish `git.diff.changed` after mutation.

- [ ] Write API tests against a temporary local Git repository for commit, push, and status refresh.
- [ ] Run the focused test to verify it fails before the handlers exist.
- [ ] Implement request types, handlers, route registration, and snapshot publication through `ToolRegistry`.
- [ ] Ensure invalid empty commit messages return `400` and tool failures return structured `500` JSON.
- [ ] Run Git API and tool-runtime tests.
- [ ] Commit `feat: expose git mutations through workspace api`.

### Task 3: Verify Protocol and Runtime Synchronization

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json` only if the event payload needs correction.
- Modify: `crates/protocol/src/lib.rs` only if schema and Rust diverge.
- Modify: `crates/server/src/workspace.rs`
- Test: `crates/protocol/src/lib.rs` and `crates/tool-runtime/src/tools/git.rs`

**Interfaces:**
- `file.changed` retains `{type, path, change, created_at}`.
- `git.diff.changed` retains `{type, status, diff, created_at}`.

- [ ] Add regression coverage proving save and Git mutation paths publish the expected event payloads.
- [ ] Run protocol and tool-runtime tests before changing implementation.
- [ ] Align schema, Rust serialization, and frontend generated types if needed using `npm run generate:protocol`.
- [ ] Run `cargo test -p evohime-protocol -p evohime-tool-runtime` and protocol generation.
- [ ] Commit `test: cover stage 4 workspace synchronization`.

### Task 4: Complete Files, Editor, and Git Panels

**Files:**
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/styles.css`

**Interfaces:**
- Files panel: lazy tree, refresh, create file, open file.
- Editor panel: Monaco language detection, dirty state, reload/save, Ctrl/Cmd+S, conflict notice.
- Git panel: status, selectable diff path, commit message, pull/push controls, operation feedback.

- [ ] Add UI state and handlers for create-file and Git actions before wiring controls.
- [ ] Run `npm run build` to capture missing typing/route contract failures.
- [ ] Implement controls with disabled/loading states and event-driven refresh.
- [ ] Render diff lines with additions/removals/context classes while preserving raw content.
- [ ] Run `npm run build` and inspect the generated bundle for a successful Vite build.
- [ ] Commit `feat: complete stage 4 workspace panels`.

### Task 5: Full Verification and Handoff

**Files:**
- Modify: `docs/current-state.md` and `docs/roadmap.md`

- [ ] Run `cargo test --workspace`.
- [ ] Run `npm run build` from `frontend/web`.
- [ ] Run `git diff --check` and inspect `git status --short --branch`.
- [ ] Update roadmap/current state to mark the completed deliverables and note any environment-only limitation.
- [ ] Commit `docs: mark stage 4 editor files and git complete`.
- [ ] Push the final branch only after fresh verification succeeds.

