import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

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

  it('does not include prompts or URLs from structured and malformed event payloads', () => {
    const files = buildSupportBundleFiles({
      snapshot: {},
      runtime: {},
      events: [
        { sequenceId: 1, eventType: 'task.failed', payload: JSON.stringify({ prompt: 'private context', error: 'https://example.test/path' }) },
        { sequenceId: 2, eventType: 'task.failed', payload: 'raw prompt https://example.test/raw' }
      ],
      logs: []
    })

    expect(files.events).not.toContain('private context')
    expect(files.events).not.toContain('example.test')
    expect(files.errors).not.toContain('private context')
    expect(files.errors).not.toContain('example.test')
  })

  it('reads bounded log files and preserves the safe Ollama fallback marker', () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-support-logs-'))
    const log = join(directory, 'shell.jsonl')
    try {
      writeFileSync(log, JSON.stringify({
        event: 'shell.ollama_download_fallback',
        error_code: 'client_blocked',
        source: 'electron_transport',
        operation: 'ollama.download',
        url: 'https://ollama.com/download/OllamaSetup.exe',
        prompt: 'private context',
        token: 'ghp_should-not-leak'
      }) + '\n', 'utf8')

      const files = buildSupportBundleFiles({ snapshot: {}, runtime: {}, events: [], logs: [log] })

      expect(files.logs).toContain('shell.ollama_download_fallback')
      expect(files.logs).toContain('client_blocked')
      expect(files.logs).toContain('ollama.download')
      expect(files.redactionReport).toMatchObject({
        observed_markers: { shell_ollama_download_fallback: true }
      })
      expect(files.logs).not.toContain('ollama.com')
      expect(files.logs).not.toContain('private context')
      expect(files.logs).not.toContain('ghp_should-not-leak')
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  })

  it('projects task failures without retaining the raw error field', () => {
    const files = buildSupportBundleFiles({
      snapshot: {},
      runtime: {},
      events: [{
        sequenceId: 3,
        eventType: 'task.failed',
        payload: JSON.stringify({
          error: 'net::ERR_BLOCKED_BY_CLIENT https://provider.test?token=secret',
          prompt: 'private context',
          operation: 'browser.navigate'
        })
      }],
      logs: []
    })

    expect(files.events).toContain('"error_code":"task_failed"')
    expect(files.events).toContain('"source":"core"')
    expect(files.events).toContain('"operation":"browser.navigate"')
    expect(files.events).not.toContain('provider.test')
    expect(files.events).not.toContain('private context')
    expect(files.events).not.toContain('"error"')
    expect(files.redactionReport).toMatchObject({
      observed_markers: { shell_ollama_download_fallback: false }
    })
  })
})
