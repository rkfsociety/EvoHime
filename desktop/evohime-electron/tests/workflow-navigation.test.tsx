import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('workflow navigation', () => {
  it('keeps workflow functionality out of the standalone App navigation and route', () => {
    const source = readFileSync(resolve(__dirname, '../src/renderer/src/App.tsx'), 'utf8')

    expect(source).not.toContain("id: 'workflows'")
    expect(source).not.toContain("view === 'workflows'")
    expect(source).not.toContain("import('./WorkflowsWorkspace')")
    expect(source).not.toContain('Составные задачи')
  })

  it('does not keep the removed draft-opening callback in the recipe panel contract', () => {
    const source = readFileSync(resolve(__dirname, '../src/renderer/src/CapabilityRecipePanel.tsx'), 'utf8')

    expect(source).not.toContain('onOpenDraft')
    expect(source).not.toContain('CapabilityRecipeForkResult')
  })
})
