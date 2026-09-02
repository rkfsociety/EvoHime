# UI components

Framework: React 19 + TypeScript. Styling is vanilla CSS in `desktop/evohime-electron/src/renderer/src/styles.css`; no component library is used.

## Shared primitives

The application uses native `button`, `input`, `textarea`, `select`, `details` and CSS classes instead of a shared primitive library. Common visual patterns are `.panel`, `.panel__header`, `.shell__panel`, `.status-pill`, `.nav-item`, and `.account__menu-item`.

## App-level components

`desktop/evohime-electron/src/renderer/src/App.tsx` composes the shell, navigation, top bar, status bar, recovery/update surfaces and routed panels.

```tsx
<div className={`shell${traceOpen ? ' shell--trace-open' : ''}${sidebarCollapsed ? ' shell--sidebar-collapsed' : ''}`}>
  <nav className="sidebar" aria-label="Разделы">...</nav>
  <main className="main">
    <header className="topbar">...</header>
    <div className="main__body">...</div>
  </main>
  <footer className="statusbar">...</footer>
</div>
```

`ProjectSidebar`, `TaskTimeline`, `WorkbenchPanel`, `SettingsModal`, `OverviewPanel` and the feature panels are page-specific components imported by `App.tsx`.
