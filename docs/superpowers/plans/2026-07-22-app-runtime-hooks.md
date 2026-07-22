# App Runtime Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the monolithic React runtime in `app.tsx` into `useWebSocket`, `useChat`, and `useWorkspace` hooks without changing user-visible behavior.

**Architecture:** Keep `app.tsx` as the composition and rendering shell. `useWebSocket` owns socket lifecycle and command transport, `useChat` owns sessions/messages/composer state, and `useWorkspace` owns project/file/Git/bootstrap state. Existing API modules and `useServerEventHandler` remain the integration boundaries.

**Tech Stack:** React hooks, TypeScript, WebSocket, existing EvoHime API clients.

## Global Constraints

- No business logic is added to the frontend.
- Preserve existing event semantics and public panel props.
- Do not edit generated protocol types by hand.
- Do not create a repository branch.
- Verify with `npm run typecheck` and `npm run build` from `frontend/web`.

---

### Task 1: Extract socket lifecycle and transport

**Files:**
- Create: `frontend/web/src/hooks/useWebSocket.ts`
- Modify: `frontend/web/src/app.tsx`

Move WebSocket URL creation, reconnect timers, sequence cursor, bootstrap history handling, and command sending behind a typed hook. Keep the current `ServerEvent` callback and connection-state behavior unchanged.

- [x] Add the hook and replace direct socket lifecycle code in `App`.
- [x] Run `npm run typecheck`.

### Task 2: Extract chat/session state

**Files:**
- Create: `frontend/web/src/hooks/useChat.ts`
- Modify: `frontend/web/src/app.tsx`

Move session selection, session hydration, chat lines, streaming text, input, attachments, send/stop/copy/export handlers, and chat notices into the hook. Expose the exact values and callbacks consumed by the existing JSX.

- [x] Add the hook and replace the corresponding state/handlers in `App`.
- [x] Run `npm run typecheck`.

### Task 3: Extract workspace state and bootstrap loading

**Files:**
- Create: `frontend/web/src/hooks/useWorkspace.ts`
- Modify: `frontend/web/src/app.tsx`

Move project picker, model/composer routes, file tree/editor state, Git state/actions, permission/tool/MCP/GitHub bootstrap, and pull-request loading into the hook. Keep panel callbacks compatible with current panel components.

- [x] Add the hook and replace the corresponding state/effects/handlers in `App`.
- [x] Run `npm run typecheck` and `npm run build`.

### Task 4: Update documentation and commit

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`

Mark `7.69` complete with the actual hook boundaries and record the next planned item.

- [x] Run `git diff --check` and verify the generated protocol file is unchanged.
- [x] Commit related changes with `git commit -m "refactor(ui): extract app runtime hooks"`.
