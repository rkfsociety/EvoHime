import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { resolveUiEntry } from '../src/main/ui-bundle'

const root = join(process.cwd(), '.tmp-ui-bundle-test')
afterEach(() => rmSync(root, { recursive: true, force: true }))

describe('resolveUiEntry', () => {
  it('selects a bounded active version', () => {
    mkdirSync(join(root, 'ui-bundles', '1.2.3'), { recursive: true })
    writeFileSync(join(root, 'ui-bundles', '1.2.3', 'index.html'), 'ok')
    writeFileSync(join(root, 'ui-active.json'), JSON.stringify({ version: '1.2.3' }))
    expect(resolveUiEntry({ root, fallback: join(root, 'bundled.html') })).toBe(join(root, 'ui-bundles', '1.2.3', 'index.html'))
  })

  it('falls back on corrupt or incomplete pointer', () => {
    mkdirSync(root, { recursive: true })
    const fallback = join(root, 'bundled.html')
    writeFileSync(fallback, 'fallback')
    writeFileSync(join(root, 'ui-active.json'), JSON.stringify({ version: '../secrets' }))
    expect(resolveUiEntry({ root, fallback })).toBe(fallback)
  })
})
