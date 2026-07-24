# 7.104 Mobile-Responsive Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the EvoHime workspace shell (sidebar + main panel + task-trace panel) usable at `≤768px` by collapsing the sidebar and trace panel into off-canvas drawers, opened via a hamburger button, without touching any individual panel's internal layout.

**Architecture:** Pure CSS media-query collapse of the existing `.workspace` grid plus one new `sidebarOpen` boolean in `app.tsx`. The sidebar (`nav.sidebar`, always mounted) gets a JS-toggled class that CSS translates on/off screen at `≤768px`; the trace panel (`aside.traceSidebar`, conditionally mounted on the existing `traceOpen` state) gets fixed positioning at the same breakpoint — no new state needed for it. A shared backdrop closes whichever drawer is open on click; Escape does the same. No per-panel (Editor/Terminal/Git/...) changes.

**Tech Stack:** React + TypeScript (`frontend/web/src/app.tsx`), plain CSS (`frontend/web/src/styles/*.css`, plain `@import`, no CSS modules/preprocessor). No test runner exists for the frontend (`frontend/web/package.json` has only `dev`/`typecheck`/`build`/`preview` — no vitest/jest/playwright configured), so verification for this plan is `tsc` typecheck + `vite build` + manual browser check, not automated tests.

## Global Constraints

- Mobile breakpoint is `≤768px` (spec: [2026-07-24-mobile-responsive-shell-design.md](../specs/2026-07-24-mobile-responsive-shell-design.md)).
- Touch targets at `≤768px` must be ≥44×44px: `.sendButton`, sidebar row buttons (`.quickLink`, `.projectCard`, `.projectChatItem`, `.standaloneSidebarChat`, `.taskSummaryCard`), the new hamburger button, `.traceToggle`, `.traceClose`.
- No changes to any panel under `frontend/web/src/panels/` — shell-only.
- No push — commit locally per repo rule (`AGENTS.md` rule 11); never run `git push`.
- Don't touch `.github/workflows/rust.yml` — this is a frontend-only change.
- Reuse existing `traceOpen` state (`useWorkspace` hook) for the trace drawer; do not add a second state for it.

---

## File Structure

- Create: `frontend/web/src/styles/mobile-shell.css` — hamburger button, `@media (max-width: 768px)` off-canvas rules for `.sidebar`/`.traceSidebar`, shared `.drawerBackdrop`, touch-target bumps. One file, one responsibility (mobile shell chrome), following the existing pattern of `memory-responsive.css` being a dedicated responsive-override file.
- Modify: `frontend/web/src/styles.css:8` — add the new import.
- Modify: `frontend/web/src/app.tsx` — add `sidebarOpen` state, hamburger button in `.topBar`, shared backdrop element, Escape-key handling, sidebar auto-close on inner click, `id`/`aria-*` wiring.

---

### Task 1: Off-canvas CSS (drawers + backdrop + touch targets)

**Files:**
- Create: `frontend/web/src/styles/mobile-shell.css`
- Modify: `frontend/web/src/styles.css`

**Interfaces:**
- Consumes: existing class names `.sidebar`, `.traceSidebar`, `.workspace`, `.workspace.traceOpen`, `.sendButton`, `.quickLink`, `.projectCard`, `.projectChatItem`, `.standaloneSidebarChat`, `.taskSummaryCard`, `.traceToggle`, `.traceClose` (all already defined in `core.css`/`navigation.css`/`workspace.css`).
- Produces: new class names `.hamburgerButton`, `.sidebarOpen` (modifier applied to `.sidebar`), `.drawerBackdrop` — these are the exact names Task 2's JSX must use.

- [ ] **Step 1: Create the CSS file**

Write `frontend/web/src/styles/mobile-shell.css`:

```css
.hamburgerButton {
  display: none;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border: 1px solid var(--border-0);
  border-radius: 12px;
  color: var(--text-1);
  background: rgba(255, 255, 255, 0.04);
  font-size: 18px;
  cursor: pointer;
}

.hamburgerButton:hover {
  border-color: rgba(143, 180, 255, 0.4);
  color: var(--text-0);
  background: rgba(91, 134, 255, 0.14);
}

.drawerBackdrop {
  display: none;
}

@keyframes mobileDrawerSlideIn {
  from {
    transform: translateX(100%);
  }
  to {
    transform: translateX(0);
  }
}

@media (max-width: 768px) {
  .hamburgerButton {
    display: inline-flex;
  }

  .workspace,
  .workspace.traceOpen {
    grid-template-columns: 1fr;
  }

  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: min(320px, 84vw);
    z-index: 25;
    border-radius: 0 24px 24px 0;
    transform: translateX(-100%);
    transition: transform 0.22s ease;
  }

  .sidebar.sidebarOpen {
    transform: translateX(0);
  }

  .traceSidebar {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(320px, 84vw);
    z-index: 25;
    border-radius: 24px 0 0 24px;
    animation: mobileDrawerSlideIn 0.22s ease;
  }

  .drawerBackdrop {
    display: block;
    position: fixed;
    inset: 0;
    z-index: 24;
    background: rgba(5, 9, 18, 0.6);
  }

  .sendButton {
    width: 44px;
    height: 44px;
  }

  .quickLink,
  .projectCard,
  .projectChatItem,
  .standaloneSidebarChat,
  .taskSummaryCard {
    min-height: 44px;
  }

  .hamburgerButton,
  .traceToggle,
  .traceClose {
    min-width: 44px;
    min-height: 44px;
  }
}
```

- [ ] **Step 2: Import it**

Edit `frontend/web/src/styles.css` — current content is:

```css
@import "./styles/core.css";
@import "./styles/navigation.css";
@import "./styles/settings.css";
@import "./styles/chat.css";
@import "./styles/workspace.css";
@import "./styles/panels.css";
@import "./styles/plugins-sites.css";
@import "./styles/memory-responsive.css";
```

Add a final line so it reads:

```css
@import "./styles/core.css";
@import "./styles/navigation.css";
@import "./styles/settings.css";
@import "./styles/chat.css";
@import "./styles/workspace.css";
@import "./styles/panels.css";
@import "./styles/plugins-sites.css";
@import "./styles/memory-responsive.css";
@import "./styles/mobile-shell.css";
```

- [ ] **Step 3: Typecheck/build sanity (CSS can't break TS, but confirm the build pipeline still picks it up)**

Run: `cd frontend/web && npm run build`
Expected: build succeeds (exit code 0), same as before this change — this only confirms Vite still bundles CSS cleanly, not the visual result (visual result is verified in Task 3).

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/styles/mobile-shell.css frontend/web/src/styles.css
git commit -m "feat(web): add off-canvas mobile shell CSS for 7.104"
```

---

### Task 2: Wire hamburger button, drawer state, backdrop, and Escape handling in app.tsx

**Files:**
- Modify: `frontend/web/src/app.tsx`

**Interfaces:**
- Consumes: `.hamburgerButton`, `.sidebarOpen`, `.drawerBackdrop` classes from Task 1. Existing `traceOpen`/`setTraceOpen` from `workspaceState` (already destructured in `app.tsx`, see the `useWorkspace` destructure block near the top of the component).
- Produces: nothing new consumed by later tasks — this is the last task.

- [ ] **Step 1: Add `sidebarOpen` state**

In `frontend/web/src/app.tsx`, find this existing line (in the block of `useState` declarations after `workspaceState` is destructured):

```tsx
  const [settingsOpen, setSettingsOpen] = useState(false);
```

Change it to:

```tsx
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
```

- [ ] **Step 2: Close both drawers on Escape**

Find the `useEffect` that handles `popstate`/other top-level effects is inside `useWorkspace`, not `app.tsx` — so add a fresh effect directly in `App()`. Place it right after the `reportWorkspaceError` callback definition:

```tsx
  const reportWorkspaceError = useCallback((message: string) => {
    setLines((current) => [...current, createChatLine({ role: "system", text: message })]);
  }, [setLines]);
```

Add immediately after it:

```tsx
  useEffect(() => {
    if (!sidebarOpen && !traceOpen) {
      return;
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setSidebarOpen(false);
        setTraceOpen(false);
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [sidebarOpen, traceOpen, setTraceOpen]);
```

`useEffect` is already imported at the top of `app.tsx` (`import { ChangeEvent, FormEvent, Fragment, UIEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";`), so no new import is needed.

- [ ] **Step 3: Add the hamburger button to the top bar**

Find:

```tsx
      <header className="topBar">
        <AgentBrand />
        <button
          type="button"
          className={traceOpen ? "traceToggle active" : "traceToggle"}
          onClick={() => setTraceOpen((open) => !open)}
          aria-expanded={traceOpen}
          aria-controls="task-trace"
          aria-label="Показать или скрыть боковую панель трейса"
        >
          <span aria-hidden="true">⌁</span>
          Трейс
        </button>
```

Replace with:

```tsx
      <header className="topBar">
        <AgentBrand />
        <button
          type="button"
          className="hamburgerButton"
          onClick={() => setSidebarOpen((open) => !open)}
          aria-expanded={sidebarOpen}
          aria-controls="workspace-sidebar"
          aria-label="Показать или скрыть боковое меню"
        >
          <span aria-hidden="true">☰</span>
        </button>
        <button
          type="button"
          className={traceOpen ? "traceToggle active" : "traceToggle"}
          onClick={() => setTraceOpen((open) => !open)}
          aria-expanded={traceOpen}
          aria-controls="task-trace"
          aria-label="Показать или скрыть боковую панель трейса"
        >
          <span aria-hidden="true">⌁</span>
          Трейс
        </button>
```

- [ ] **Step 4: Add the shared backdrop and wire the sidebar's `id`/class/auto-close**

Find:

```tsx
      <section className={traceOpen ? "workspace traceOpen" : "workspace"}>
        <nav className="sidebar">
```

Replace with:

```tsx
      {(sidebarOpen || traceOpen) ? (
        <div
          className="drawerBackdrop"
          onClick={() => {
            setSidebarOpen(false);
            setTraceOpen(false);
          }}
          aria-hidden="true"
        />
      ) : null}

      <section className={traceOpen ? "workspace traceOpen" : "workspace"}>
        <nav
          className={sidebarOpen ? "sidebar sidebarOpen" : "sidebar"}
          id="workspace-sidebar"
          onClick={(event) => {
            if (sidebarOpen && (event.target as HTMLElement).closest("button")) {
              setSidebarOpen(false);
            }
          }}
        >
```

This closes the sidebar drawer whenever any button inside it is clicked (nav links, project/chat rows, archive buttons, settings gear) — matching the "selecting a destination closes the drawer" requirement from the spec without editing each of the ~8 individual `onClick` handlers inside the sidebar.

- [ ] **Step 5: Typecheck**

Run: `cd frontend/web && npm run typecheck`
Expected: exits 0, no new TS errors (the new code uses only existing imported hooks/types; `event.target as HTMLElement` is a standard DOM cast).

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/app.tsx
git commit -m "feat(web): add mobile hamburger drawer + backdrop for shell (7.104)"
```

---

### Task 3: Manual verification in a real browser

**Files:** none (verification only)

**Interfaces:**
- Consumes: everything from Task 1 + Task 2.

- [ ] **Step 1: Start the frontend dev server**

Run: `.\start-dev.ps1 -Web` (per repo rule, always use this launcher, not a bare `npm run dev`) — or if a Rust backend isn't needed for this visual check, `cd frontend/web && npm run dev` is acceptable for a frontend-only smoke test since no data-dependent behavior is being tested. Prefer `start-dev.ps1 -Web` if in doubt.

- [ ] **Step 2: Desktop width baseline (no regression)**

Open the app at ≥1280px width. Confirm:
- Sidebar renders inline as the first grid column (unchanged from before).
- No hamburger button visible in `.topBar`.
- Clicking "Трейс" still opens the trace panel as a third grid column (unchanged).

- [ ] **Step 3: Mobile width — sidebar drawer**

Resize/emulate to 600px width (or 768px exactly, then just under it). Confirm:
- Hamburger button is visible in `.topBar`, sidebar is off-screen by default, main panel (chat) fills the full width.
- Clicking the hamburger slides the sidebar in from the left with a dark backdrop behind it.
- Clicking the backdrop closes the sidebar.
- Pressing Escape while the sidebar is open closes it.
- Clicking any sidebar row (e.g. a quick link or a chat) closes the sidebar automatically.

- [ ] **Step 4: Mobile width — trace drawer**

At the same mobile width, open a task so the "Трейс" button is meaningful, then:
- Click "Трейс" — the trace panel appears as a right-side overlay with a backdrop, not a third grid column.
- Backdrop click and Escape both close it.
- Confirm the sidebar and trace drawer don't fight each other if one is opened while the other is already open (backdrop click should close whichever is open; per Task 2 Step 4 the shared backdrop closes both unconditionally, which is acceptable — confirm this doesn't feel broken in practice).

- [ ] **Step 5: Touch target spot-check**

At mobile width, use browser dev tools (or the `read_page`/`zoom` tools if verifying via the in-app Browser pane) to confirm `.sendButton`, sidebar row buttons, the hamburger, and `.traceToggle`/`.traceClose` each measure at least 44×44px.

- [ ] **Step 6: Final build check**

Run: `cd frontend/web && npm run build`
Expected: exits 0.

- [ ] **Step 7: Commit if any fixes were needed during verification**

If Steps 2–6 required code fixes, stage and commit them with a message describing the fix (e.g. `fix(web): correct mobile drawer z-index stacking`). If no fixes were needed, no commit is required for this task.
