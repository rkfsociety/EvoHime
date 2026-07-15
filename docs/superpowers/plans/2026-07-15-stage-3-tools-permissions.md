# Stage 3 Tools and Permissions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать безопасные filesystem/shell-инструменты, approval lifecycle и браузерный Terminal для этапа 3 EvoHime.

**Architecture:** Общий sandbox в `evohime-tool-runtime` проверяет canonical path и запускает инструменты через registry. `evohime-permissions` принимает решение ask/allow/deny и хранит одноразовые approvals; server связывает их с WebSocket-сессией. Frontend только отображает события и отправляет approval-команды.

**Tech Stack:** Rust 2021, Tokio, Axum WebSocket, Serde/JSON Schema, React + TypeScript + Vite, xterm.js.

## Global Constraints

- Все filesystem paths ограничены `WORKSPACE_ROOT`, включая symlink traversal.
- `shell.execute` запускает процесс без `cmd /c` или `sh -c`, с cwd внутри workspace.
- Write и shell по умолчанию требуют approval; sandbox не обходится постоянной политикой.
- `protocol.generated.ts` генерируется скриптом и не редактируется вручную.
- UI не содержит бизнес-логику инструментов.
- Каждый task заканчивается targeted test/build и отдельным commit.

### Task 1: Shared workspace sandbox and permission-aware registry

**Files:**
- Create: `crates/tool-runtime/src/sandbox.rs`
- Modify: `crates/tool-runtime/src/lib.rs`
- Modify: `crates/tool-runtime/src/registry.rs`
- Modify: `crates/permissions/src/lib.rs`
- Test: `crates/tool-runtime/src/sandbox.rs`, `crates/tool-runtime/src/registry.rs`

**Interfaces:**
- `WorkspaceSandbox::resolve_existing(path: &str) -> Result<PathBuf, ToolError>`
- `WorkspaceSandbox::resolve_for_write(path: &str) -> Result<PathBuf, ToolError>`
- `PermissionMode::{Ask, Allow, Deny}`
- `PermissionDecision::{Allowed, NeedsApproval, Denied}`
- `ToolContext { workspace_root, permissions }`

- [ ] **Step 1: Add failing sandbox tests** for normal files, `..`, absolute paths, symlink escape, and a new-file path whose parent is inside root.
- [ ] **Step 2: Run** `cargo test -p evohime-tool-runtime sandbox`; expected: compilation/test failure because sandbox APIs do not exist.
- [ ] **Step 3: Implement canonical root and parent-canonicalization checks**; reject any resolved path not starting with the canonical root.
- [ ] **Step 4: Add permission policy/decision tests** for ask, allow, deny and default policies.
- [ ] **Step 5: Wire registry permission checking** before dispatch and add `ToolError::NeedsApproval` with approval metadata.
- [ ] **Step 6: Run** `cargo test -p evohime-permissions`; then run `cargo test -p evohime-tool-runtime`; expected: PASS.
- [ ] **Step 7: Commit** `feat: add workspace sandbox and permission gate`.

### Task 2: filesystem.write

**Files:**
- Modify: `crates/tool-runtime/src/tools/filesystem.rs`
- Modify: `crates/tool-runtime/src/tools/mod.rs`
- Modify: `crates/tool-runtime/src/registry.rs`
- Create: `crates/tool-runtime/schemas/filesystem.write.json`
- Test: `crates/tool-runtime/src/tools/filesystem.rs`

**Interfaces:**
- `pub const WRITE_NAME: &str = "filesystem.write"`
- Input: `{ "path": string, "content": string }`
- Structured result: `{ path, bytes, change: "created" | "updated" }`

- [ ] **Step 1: Write failing tests** for create, overwrite, nested parent creation, traversal rejection, and invalid input.
- [ ] **Step 2: Run** `cargo test -p evohime-tool-runtime filesystem::tests::writes`; expected: FAIL.
- [ ] **Step 3: Implement write through `resolve_for_write`**, create parent directories only after validation, write UTF-8 content, and report change type.
- [ ] **Step 4: Register the tool with `FilesystemWrite` and a 10-second timeout.**
- [ ] **Step 5: Run** `cargo test -p evohime-tool-runtime filesystem`; then run `cargo test -p evohime-tool-runtime`; commit `feat: add filesystem write tool`.

### Task 3: filesystem.patch and filesystem.search

**Files:**
- Create: `crates/tool-runtime/src/tools/patch.rs`
- Create: `crates/tool-runtime/src/tools/search.rs`
- Modify: `crates/tool-runtime/src/tools/mod.rs`
- Modify: `crates/tool-runtime/src/registry.rs`
- Create: `crates/tool-runtime/schemas/filesystem.patch.json`
- Create: `crates/tool-runtime/schemas/filesystem.search.json`
- Test: `crates/tool-runtime/src/tools/patch.rs`, `crates/tool-runtime/src/tools/search.rs`

**Interfaces:**
- Patch input: `{ "path": string, "patch": string }`; result: `{ path, hunks_applied, bytes }`.
- Search input: `{ "query": string, "path"?: string, "glob"?: string, "limit"?: number }`; result contains `matches[]` with path, line, text.

- [ ] **Step 1: Write failing patch tests** for one hunk, multiple hunks, context mismatch, malformed diff, and traversal.
- [ ] **Step 2: Implement a line-based unified-diff parser/applicator** without invoking shell; write only after every hunk validates.
- [ ] **Step 3: Write failing search tests** for recursive matches, path restriction, glob, limit, no matches, and traversal.
- [ ] **Step 4: Implement `rg --json --no-heading --color never` via `tokio::process::Command`**, fixed workspace cwd, validated path/glob arguments, bounded output, and structured parsing.
- [ ] **Step 5: Register both tools with read/write permissions and timeouts.**
- [ ] **Step 6: Run** `cargo test -p evohime-tool-runtime`; commit `feat: add filesystem patch and search tools`.

### Task 4: shell.execute, process limits, timeout and cancellation

**Files:**
- Create: `crates/tool-runtime/src/tools/shell.rs`
- Modify: `crates/tool-runtime/src/tools/mod.rs`
- Modify: `crates/tool-runtime/src/registry.rs`
- Create: `crates/tool-runtime/schemas/shell.execute.json`
- Test: `crates/tool-runtime/src/tools/shell.rs`, `crates/tool-runtime/src/registry.rs`

**Interfaces:**
- Input: `{ "program": string, "args"?: string[], "cwd"?: string, "timeout_ms"?: number }`.
- Structured result: `{ cwd, stdout, stderr, exit_code, timed_out }`.

- [ ] **Step 1: Write failing tests** for direct executable invocation, cwd restriction, traversal, stdout/stderr, non-zero exit, timeout and cancellation.
- [ ] **Step 2: Implement direct `Command::new(program).args(args)`** with validated cwd, bounded stdout/stderr, and process kill on timeout.
- [ ] **Step 3: Integrate the caller timeout and `CancellationToken` so cancellation returns `ToolError::Cancelled` and terminates the child.
- [ ] **Step 4: Register `ShellExecute` with approval-required default and an explicit maximum timeout.
- [ ] **Step 5: Run** `cargo test -p evohime-tool-runtime`; commit `feat: add sandboxed shell tool`.

### Task 5: Approval protocol and server lifecycle

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Modify: `crates/server/src/app.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `crates/server/src/workspace.rs`
- Modify: `crates/server/Cargo.toml`
- Regenerate: `frontend/web/src/protocol.generated.ts`
- Modify: `frontend/web/src/protocol.ts`
- Test: `crates/protocol/src/lib.rs`, `crates/server/tests/approval_ws.rs`

**Interfaces:**
- Event `ApprovalRequired { approval_id, task_id, tool_name, permission, scope, created_at }`.
- Commands `ApprovalGranted { approval_id }` and `ApprovalDenied { approval_id }`.
- `ApprovalStore::create/get/resolve` with one-shot state transition.

- [ ] **Step 1: Add protocol serialization round-trip tests** for event and both commands.
- [ ] **Step 2: Update schema and Rust enums**, then run `npm run generate:protocol` from repository root.
- [ ] **Step 3: Add approval store tests** for pending, grant, deny, duplicate resolve and unknown ids.
- [ ] **Step 4: Wire `AppState` with `Arc<PermissionEngine>` and approval store; route client commands without crashing the socket.
- [ ] **Step 5: Publish `approval.required` to history/session bus and resume or fail the waiting task on grant/deny.
- [ ] **Step 6: Run** `cargo test -p evohime-protocol -p evohime-server`; then run `npm run generate:protocol`; commit `feat: add approval protocol lifecycle`.

### Task 6: Terminal output and approval modal

**Files:**
- Modify: `frontend/web/package.json`
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/styles.css`
- Modify: `frontend/web/src/protocol.ts`
- Create: `frontend/web/src/components/TerminalPanel.tsx`
- Create: `frontend/web/src/components/ApprovalModal.tsx`
- Test: `frontend/web/src/components/TerminalPanel.test.tsx`, `frontend/web/src/components/ApprovalModal.test.tsx`

**Interfaces:**
- `TerminalPanel({ entries })` renders stdout/stderr and exit status.
- `ApprovalModal({ request, onGrant, onDeny })` renders tool/scope and emits approval command callbacks.

- [ ] **Step 1: Add xterm.js, Vitest, and Testing Library dependencies plus `vitest.config.ts` and a jsdom setup file.**
- [ ] **Step 2: Add failing component tests or deterministic reducer tests** for approval-required state, grant/deny payloads, and shell output accumulation.
- [ ] **Step 3: Implement terminal state derived from `tool.output` and `tool.completed`; keep raw events as the source of truth.**
- [ ] **Step 4: Implement modal and WebSocket command handlers; never render untrusted scope as executable HTML.**
- [ ] **Step 5: Replace Terminal placeholder and add approval modal overlay; from `frontend/web` run** `npm run build`.
- [ ] **Step 6: Commit** `feat: add terminal and approval UI`.

### Task 7: Permission settings

**Files:**
- Modify: `crates/permissions/src/lib.rs`
- Modify: `crates/server/src/app.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/styles.css`
- Test: permissions and server settings tests

**Interfaces:**
- GET `/api/permissions` returns permission names and current modes.
- PUT `/api/permissions/:permission` accepts `{ "mode": "ask" | "allow" | "deny" }`.

- [ ] **Step 1: Write failing policy serialization and endpoint tests** for valid modes, unknown permission, and invalid mode.
- [ ] **Step 2: Implement policy snapshot/update with validation and no filesystem access.**
- [ ] **Step 3: Render a settings table with explicit mode selectors and server error states.**
- [ ] **Step 4: Run** `cargo test --workspace`; then from `frontend/web` run `npm run build`; commit `feat: add permission settings`.

### Task 8: Roadmap bookkeeping and end-to-end verification

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Test: repository verification commands

- [ ] **Step 1: Run** `cargo fmt --all -- --check`.
- [ ] **Step 2: Run** `cargo test --workspace`.
- [ ] **Step 3: Run** `npm run generate:protocol` and verify generated diff is intentional.
- [ ] **Step 4: From `frontend/web`, run** `npm run build`.
- [ ] **Step 5: Execute a real local smoke flow**: start server with a temp workspace, issue write/search/shell calls, verify approval.required, grant/deny, and inspect returned filesystem/output state.
- [ ] **Step 6: Mark deliverables 3.1–3.11 complete only when the smoke flow and tests pass; run `git diff --check` and verify no generated file was hand-edited.**
- [ ] **Step 7: Commit** `docs: mark stage 3 complete`.

## Verification Matrix

| Requirement | Evidence |
| --- | --- |
| filesystem sandbox | Rust tests for traversal, symlink, write parent |
| write/patch/search | Dedicated tool tests and structured results |
| shell sandbox | Direct command, cwd, timeout, cancellation tests |
| approval lifecycle | Protocol round-trip and WebSocket tests |
| Terminal | Frontend build plus component/reducer tests |
| settings | Endpoint/policy tests plus frontend build |
| readiness criterion | Real smoke flow with state/output inspection |
