import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const testDirectory = dirname(fileURLToPath(import.meta.url))
const stylesheet = readFileSync(resolve(testDirectory, '../src/renderer/src/styles.css'), 'utf8')
const projectSidebar = readFileSync(resolve(testDirectory, '../src/renderer/src/ProjectSidebar.tsx'), 'utf8')
const taskTimeline = readFileSync(resolve(testDirectory, '../src/renderer/src/TaskTimeline.tsx'), 'utf8')
const appSource = readFileSync(resolve(testDirectory, '../src/renderer/src/App.tsx'), 'utf8')

describe('responsive shell layout contract', () => {
  it('uses a dedicated icon rail and a separate workspace/chat column', () => {
    expect(stylesheet).toContain('grid-template-columns: 68px minmax(0, 1fr);')
    expect(stylesheet).toContain('.sidebar__nav {')
    expect(stylesheet).toContain('.sidebar__projects {')
    expect(stylesheet).toContain('.sidebar__nav .nav-item__label')
  })

  it('moves workspace selection into the context rail and removes duplicate composer controls', () => {
    expect(projectSidebar).toContain("api.invoke('workspace.select'")
    expect(projectSidebar).toContain("api.invoke('workspace.pick'")
    expect(projectSidebar).toContain('chat-rail__workspace-switcher')
    expect(taskTimeline).toContain('composer__scope')
    expect(taskTimeline).not.toContain('composer__control-group--project')
  })

  it('provides functional new-chat and settings keyboard shortcuts', () => {
    expect(appSource).toContain("event.key.toLowerCase() === 'k'")
    expect(appSource).toContain("event.code === 'Comma'")
    expect(appSource).toContain("document.getElementById('task-prompt')?.focus()")
  })

  it('keeps settings tabs keyboard-operable and exposes their panel relationship', () => {
    const source = readFileSync(resolve(testDirectory, '../src/renderer/src/SettingsModal.tsx'), 'utf8')
    expect(source).toContain('role="tablist"')
    expect(source).toContain('aria-controls="settings-panel"')
    expect(source).toContain('role="tabpanel"')
    expect(source).toContain("event.key === 'ArrowDown'")
  })
  it('uses the actual viewport as the layout boundary without CSS zoom overflow', () => {
    expect(stylesheet).toContain('width: 100%;\n  height: 100%;\n  min-width: 0;\n  min-height: 0;\n  overflow: hidden;')
    expect(stylesheet).toContain('width: 100%;\n  height: 100%;\n  min-height: 0;\n  min-width: 0;')
    expect(stylesheet).not.toContain('zoom: var(--ui-zoom, 1);')
    expect(stylesheet).not.toContain('min-width: 960px;')
  })

  it('reduces sidebar pressure at the supported minimum window width', () => {
    expect(stylesheet).toContain('@media (max-width: 1080px) {\n  :root {\n    --sidebar-width: 200px;')
  })

  it('keeps the trace panel from compressing the primary workspace', () => {
    expect(stylesheet).toContain('@media (max-width: 1180px)')
    expect(stylesheet).toContain("grid-template-areas:\n      'sidebar main'\n      'status status'")
    expect(stylesheet).toContain('position: absolute;')
    expect(stylesheet).toContain('width: min(400px, calc(100% - var(--sidebar-width) - 16px));')
  })

  it('turns auxiliary conversation panels into a bounded drawer on narrow windows', () => {
    expect(stylesheet).toContain('@media (max-width: 980px)')
    expect(stylesheet).toContain('width: min(420px, calc(100% - 20px));')
    expect(stylesheet).toContain('grid-template-columns: minmax(0, 1fr);')
  })

  it('allows the top and status bars to wrap instead of overflowing', () => {
    expect(stylesheet).toContain('@media (max-width: 1080px)')
    expect(stylesheet).toContain('.topbar {\n    height: auto;\n    min-height: 48px;\n    flex-wrap: wrap;')
    expect(stylesheet).toContain('.statusbar {\n    height: auto;\n    min-height: 26px;\n    flex-wrap: wrap;')
  })

  it('keeps row actions reachable by keyboard and touch', () => {
    expect(stylesheet).toContain('.projects__row:focus-within .projects__forget')
    expect(stylesheet).toContain('.chats__list li:focus-within .chats__remove')
    expect(stylesheet).toContain('@media (hover: none)')
    expect(stylesheet).toContain('pointer-events: auto;')
  })

  it('provides a visible focus indicator for keyboard navigation', () => {
    expect(stylesheet).toContain(':where(button, a, input, select, textarea, summary, [tabindex]):focus-visible')
    expect(stylesheet).toContain('outline: 2px solid var(--accent);')
  })
})
