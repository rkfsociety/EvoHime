import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
describe('policy aware tool result cache', () => { it('keeps cache authority in Core and exposes a bounded panel', () => { const source = readFileSync(resolve(__dirname, '../src/renderer/src/PolicyAwareToolResultCachePanel.tsx'), 'utf8'); expect(source).toContain('Never'); expect(source).toContain('core.policyAwareToolResultCache'); expect(source).not.toContain('input_payload') }) })
