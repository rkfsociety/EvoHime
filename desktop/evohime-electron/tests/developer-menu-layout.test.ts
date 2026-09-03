import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('developer menu layout', () => {
  it('keeps the grouped developer catalog in a bounded side drawer', () => {
    const styles = readFileSync(resolve(__dirname, '../src/renderer/src/styles.css'), 'utf8')
    const block = styles.match(/\.account__developer-menu\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''

    expect(block).toContain('position: fixed')
    expect(block).toContain('top: 12px')
    expect(block).toContain('bottom: 44px')
    expect(block).toContain('overflow-y: auto')
    expect(block).not.toContain('bottom: 0')
  })
})
