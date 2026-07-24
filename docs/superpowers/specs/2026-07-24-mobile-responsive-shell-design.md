# 7.104 Mobile-responsive shell — design

## Problem

`frontend/web` is browser-only by design (no Electron/native clients — [AGENTS.md](../../../AGENTS.md)), but the workspace shell is not responsive: `.workspace` is a fixed grid (`280px` sidebar + main + optional `320px` trace sidebar, [core.css](../../../frontend/web/src/styles/core.css)) with no collapse below any breakpoint. On phone/tablet widths the sidebar and trace panel either get crushed or force horizontal scroll of the whole shell. Some inner panels (Memory, Settings, Git panel table) already have partial `@media (max-width: ...)` handling, but the shell frame itself does not.

## Scope

Adaptive collapse of the existing shell at `≤768px`. No per-panel redesign, no bottom-tab navigation, no native app. Existing panels (Chat/Files/Editor/Git/Tasks/Actions/Terminal/...) keep their current internal layout and continue to render full-bleed inside `.mainPanel`; only the *shell* (sidebar, trace sidebar, navigation chrome) adapts.

## Layout change

- `.workspace` grid at `≤768px` becomes a single column: `grid-template-columns: 1fr` (via media query override), so `.mainPanel` occupies the full width.
- `.sidebar` (`nav.sidebar`) and `.traceSidebar` (`aside.traceSidebar`) switch to `position: fixed` off-canvas panels at this breakpoint:
  - `.sidebar`: fixed, full height, `left: 0`, translated off-screen (`translateX(-100%)`) by default, slides in when open.
  - `.traceSidebar`: fixed, full height, `right: 0`, translated off-screen (`translateX(100%)`) by default, slides in when `traceOpen`.
  - Both get a `z-index` above `.mainPanel` and a shared `.drawerBackdrop` (new element) behind them that dims the content and captures outside clicks to close.
- No change to `.mainPanel` internal panels — they already fill available width via `minmax(0, 1fr)`.

## Navigation state

- Reuse the existing `traceOpen` boolean (already in `app.tsx`) for the trace drawer — desktop behavior (extra grid column) is unchanged above 768px; below it, the same flag toggles the off-canvas class instead. No new state needed for trace.
- Add one new `sidebarOpen` boolean state in `app.tsx`, `false` by default. A hamburger button in `.topBar` (visible only `≤768px` via CSS) toggles it.
- Auto-close `sidebarOpen` on: backdrop click, Escape key (existing keydown handling pattern in app.tsx, extend it), and after any `navigateToPanel` call while sidebarOpen is true (selecting a destination should close the drawer).
- Above 768px, `sidebarOpen` state is inert — the CSS media query only applies the fixed/translate rules under the breakpoint, so the sidebar always renders normally on desktop regardless of the flag.

## Touch targets

At `≤768px`, bump the following interactive elements to a minimum 44×44px hit area via the same media query (padding/min-height increase, not a size change on desktop):
- `.sendButton` (currently 34×34px)
- `.quickLink`, `.projectCard`, `.projectChatItem`, `.standaloneSidebarChat`, `.taskSummaryCard` (sidebar rows, currently ~36–40px tall)
- new hamburger button and existing `.traceToggle` / `.traceClose` buttons

## Out of scope

- Per-panel mobile redesign (Editor/Monaco, Terminal, data tables) — deferred; existing partial responsive CSS (Memory, Settings, Git panel) is untouched and sufficient for now.
- Bottom tab-bar navigation — rejected in favor of drawer to minimize JS/behavior changes.
- PWA/installability, native gestures (swipe-to-open drawer) — not requested, can be a follow-up.

## Testing

- No existing frontend viewport/e2e test harness beyond typecheck/build in CI ([AGENTS.md](../../../AGENTS.md) CI list: "frontend typecheck/build"). Verification is manual: resize browser / device emulation to confirm drawer open/close, backdrop dismiss, Escape dismiss, and touch target sizes at 768px, 600px (phone), and desktop widths, plus that `npm run build` and typecheck still pass.
