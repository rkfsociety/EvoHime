# Layouts

## Application shell

- Source: `desktop/evohime-electron/src/renderer/src/App.tsx`
- Description: Three-zone desktop shell: project/chat sidebar, main workspace, persistent status bar; optional trace panel and right-side workbench/browser.

```tsx
return (
  <div className={`shell${traceOpen ? ' shell--trace-open' : ''}${sidebarCollapsed ? ' shell--sidebar-collapsed' : ''}`}>
    <nav className="sidebar" aria-label="Разделы">
      <div className="sidebar__brand">...</div>
      <div className="sidebar__projects"><ProjectSidebar ... /></div>
      <div className="account"><button className="account__user">...</button>{accountMenuOpen ? <div className="account__menu">...</div> : null}</div>
    </nav>
    <main className="main">
      <header className="topbar"><h2>{title}</h2><span>{workspace ?? 'папка не выбрана'}</span><ListeningIndicator events={events} /><span className="status-pill">{STATE_LABELS[connection]}</span></header>
      <RecoveryBanner ... />
      <div className="main__body">{view === 'chat' ? <div className="conversation-layout"><TaskTimeline ... />{workbenchVisible ? <WorkbenchPanel ... /> : null}</div> : <div className="main__scroll">...</div>}</div>
    </main>
    <footer className="statusbar">...</footer>
  </div>
)
```

## Sidebar

- Source: `desktop/evohime-electron/src/renderer/src/ProjectSidebar.tsx`
- Description: Workspace selector and chat list in the left rail; selecting a chat returns to the conversation view.

## Main conversation layout

- Source: `desktop/evohime-electron/src/renderer/src/TaskTimeline.tsx`
- Description: Main chat/task timeline with composer, messages, approvals and task state.

## Global styles

- Source: `desktop/evohime-electron/src/renderer/src/styles.css`
- Description: Vanilla CSS tokens and layout styles. Root desktop grid is `260px 1fr`, with a collapsible `58px` sidebar.
