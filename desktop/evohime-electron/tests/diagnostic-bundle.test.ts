import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { buildDiagnosticBundle, serializeDiagnosticBundle } from '../src/main/diagnostics/bundle'

describe('diagnostic bundle', () => {
  it('is bounded and redacts secrets, paths and raw workspace material', () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-diagnostics-'))
    const log = join(directory, 'shell-main.jsonl')
    try {
      writeFileSync(log, JSON.stringify({ token: 'ghp_0123456789abcdef', path: 'C:\\Users\\roman\\repo' }))
      const bundle = buildDiagnosticBundle({
        generatedAtMs: 1,
        appVersion: '1.0.0',
        platform: 'win32',
        architecture: 'x64',
        state: { connection: 'connected', reason: null },
        update: { installedCommit: 'abc123' },
        repair: { phase: 'failed', workspacePath: 'C:\\Users\\roman\\repo' },
        events: [{ sequenceId: 2, taskId: 'task-1', eventType: 'task.failed', payload: JSON.stringify({ error: 'safe error', api_key: 'secret' }) }],
        logPaths: [log]
      })
      const text = serializeDiagnosticBundle(bundle)
      expect(text).toContain('evohime-diagnostic-bundle-v1')
      expect(text).toContain('[REDACTED]')
      expect(text).not.toContain('ghp_0123456789abcdef')
      expect(text).not.toContain('C:\\Users\\roman\\repo')
      expect(text).not.toContain('workspace contents')
      expect(Buffer.byteLength(text)).toBeLessThanOrEqual(512 * 1024)
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  })
})
