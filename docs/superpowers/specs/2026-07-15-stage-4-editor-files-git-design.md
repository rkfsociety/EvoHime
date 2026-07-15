# Stage 4 Editor, Files, and Git Design

## Goal

Complete Milestone 3 / Stage 4 so a browser user can navigate the workspace, open and edit files in Monaco, save changes, inspect Git status and diffs, and run commit, pull, and push actions from the workspace UI.

## Scope

- Harden the existing workspace file API and add focused server tests.
- Keep the existing REST endpoints for file and Git reads/writes.
- Publish `file.changed` after successful saves and `git.diff.changed` after file or Git mutations.
- Add Git action controls with explicit loading, success, and error states.
- Keep `git.status`, `git.diff`, `git.commit`, `git.pull`, and `git.push` tools as the agent-facing interface.
- Do not implement Stage 5 task orchestration or general LLM tool-calling in this change.

## Architecture

The server remains the source of truth. `crates/server/src/workspace.rs` owns workspace-safe path resolution, file operations, Git snapshots, and Git mutations. Git mutations execute through the existing `ToolRegistry`, so permission checks and tool execution logs remain centralized. After a mutation, the server publishes a fresh snapshot through the session bus when a session is available.

Workspace paths are normalized as relative paths. `..`, absolute paths, Windows prefixes, and symlink escapes are rejected. Directory listings omit `.git` and return stable directory-first ordering. Existing files are canonicalized before reads; writes validate the nearest existing parent and then write only inside the workspace.

## Frontend Behavior

The Files panel loads the root directory, expands child directories lazily, refreshes after `file.changed`, and supports creating a new text file. The Editor panel opens files in Monaco, infers language from the extension, tracks dirty state, saves with the toolbar or Ctrl/Cmd+S, and warns when an external event conflicts with unsaved local edits. The Git panel shows status and a diff for the selected path or repository, and exposes commit, pull, push, and refresh controls.

Git write actions require a non-empty commit message for commit and display server/tool errors without discarding editor state. Pull and push use optional remote and branch fields. While an action is running, its controls are disabled to prevent duplicate operations.

## Events and Synchronization

Successful saves emit `file.changed` and then `git.diff.changed` for the session. Successful Git mutations emit `git.diff.changed` for the session. The frontend consumes both events, refreshes the affected directory and snapshot, and keeps the currently opened file intact unless it has no unsaved edits.

## Error Handling

Invalid or missing paths return HTTP 400. Missing files and directories return a clear HTTP 404-style API error. Tool failures are returned as structured JSON errors with the operation name. Event publication failures are logged but do not turn an already successful filesystem or Git mutation into a failed mutation.

## Testing

- Unit tests cover path normalization, traversal rejection, hidden `.git` filtering, and write behavior.
- Protocol tests cover event serialization and round trips.
- Tool tests cover Git status, diff, commit, pull, and push.
- Frontend TypeScript build verifies component and protocol typing.
- Rust workspace tests and frontend build are the completion gates.
