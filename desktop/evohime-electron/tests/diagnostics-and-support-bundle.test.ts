import { describe, expect, it } from 'vitest'

import { buildSupportBundleFiles, serializeSupportBundle } from '../src/main/diagnostics/support-bundle'

describe('support bundle v2', () => {
  it('contains bounded sections, issue draft and redaction metadata in a ZIP', () => {
    const files = buildSupportBundleFiles({
      snapshot: { schema_version: 2, health: [{ id: 'storage', status: 'PASS' }], api_key: 'ghp_should-not-leak' },
      runtime: { platform: 'win32', path: 'C:\\Users\\roman\\repo' },
      events: [{ sequenceId: 1, eventType: 'task.failed', payload: JSON.stringify({ token: 'ghp_should-not-leak' }) }],
      logs: ['safe diagnostic']
    })
    const archive = serializeSupportBundle(files)
    expect(archive.subarray(0, 4).toString('hex')).toBe('504b0304')
    expect(files.manifest.schema).toBe('evohime-support-bundle-v2')
    expect(files.redactionReport.raw_values_included).toBe(false)
    expect(files.issueDraft).toContain('### Problem')
    expect(archive.toString('utf8')).not.toContain('ghp_should-not-leak')
  })

  it('fails closed when a final archive still contains a credential', () => {
    const files = buildSupportBundleFiles({ snapshot: {}, runtime: {}, events: [], logs: [] })
    expect(() => serializeSupportBundle({ ...files, issueDraft: 'bearer leaked-value' })).toThrow('final redaction scan')
  })
})
