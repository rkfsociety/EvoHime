# Extractable components

## Layout

### AppShell
- Source: `desktop/evohime-electron/src/renderer/src/App.tsx`
- Category: layout
- Description: Main Electron application shell with sidebar, top bar, content and status bar.
- Extractable props: active view, sidebar collapsed, trace open, workbench visibility.

### ProjectSidebar
- Source: `desktop/evohime-electron/src/renderer/src/ProjectSidebar.tsx`
- Category: layout
- Description: Workspace and chat navigation rail.
- Extractable props: connection, workspace, chat id, revision.

### Topbar
- Source: `desktop/evohime-electron/src/renderer/src/App.tsx`
- Category: layout
- Description: Current view title, workspace context, controls and connection indicators.
- Extractable props: title, workspace, connection, listening state.

## Basic patterns

### StatusPill
- Source: `desktop/evohime-electron/src/renderer/src/App.tsx`
- Category: basic
- Description: Compact connection/listening status badge.
- Extractable props: status and label.

### Panel
- Source: `desktop/evohime-electron/src/renderer/src/styles.css`
- Category: basic
- Description: Raised content surface used by feature panels.
- Extractable props: none; CSS-driven.
