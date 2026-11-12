import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const stylesheet = readFileSync(
  resolve(dirname(fileURLToPath(import.meta.url)), '../src/renderer/src/styles.css'),
  'utf8'
)

describe('responsive shell layout contract', () => {
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

  it('provides a visible focus indicator for keyboard navigation', () => {
    expect(stylesheet).toContain(':where(button, a, input, select, textarea, summary, [tabindex]):focus-visible')
    expect(stylesheet).toContain('outline: 2px solid var(--accent);')
  })
})
