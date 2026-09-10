import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('navigation boundary', () => {
  it('exposes only user task surfaces and settings', () => {
    const app = readFileSync(resolve(__dirname, '../src/renderer/src/App.tsx'), 'utf8')

    expect(app).not.toContain('Интерфейс разработчика')
    expect(app).not.toContain('DEVELOPER_GROUPS')
    expect(app).not.toContain("id: 'background-execution'")
    expect(app).not.toContain("view === 'background-execution'")
    expect(app).not.toContain("id: 'plan-artifacts'")
    expect(app).not.toContain("view === 'plan-artifacts'")
  })

  it('does not expose Core-only incremental change protocol in the UI', () => {
    const app = readFileSync(resolve(__dirname, '../src/renderer/src/App.tsx'), 'utf8')

    expect(app).not.toContain("id: 'incremental-change'")
    expect(app).not.toContain("view === 'incremental-change'")
  })
})
