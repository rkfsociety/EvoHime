import { describe, expect, it, vi } from 'vitest'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { reportUpdateFailure } from '../src/main/update/update-issue-reporter'
import type { UpdateConfig } from '../src/main/update/config'
import type { UpdateStatus } from '@shared/update'

const status: UpdateStatus = {
  phase: 'failed', message: 'Ошибка', error: 'Error: token ghp_secret A:\\private\\file',
  branch: 'main', installedCommit: 'a'.repeat(40), remoteCommit: 'b'.repeat(40),
  selectedComponents: ['ui-bundle'], evidence: [], steps: [], blocking: false,
  restartRequired: false, downloadProgress: null, downloadedBytes: 0, totalBytes: null,
  detail: '', checkedAtMs: null
}

function config(stateDirectory: string): UpdateConfig {
  return { enabled: true, repositoryUrl: 'https://github.com/rkfsociety/EvoHime.git', branch: 'main', launchPolicy: 'installer', checkIntervalMs: 300000, requireGreenCommit: true, greenCommitDepth: 10, githubToken: null, sourceDirectory: 'C:\\source', stagingDirectory: 'C:\\staging', stateDirectory, installDirectory: 'C:\\install' }
}

describe('update issue reporter', () => {
  it('creates a redacted diagnostic issue and deduplicates it', async () => {
    const stateDirectory = mkdtempSync(join(tmpdir(), 'evohime-issue-'))
    const fetch = vi.fn(async () => new Response(JSON.stringify({ html_url: 'https://github.com/rkfsociety/EvoHime/issues/1' }), { status: 201, headers: { 'content-type': 'application/json' } }))
    const deps = { fetch: fetch as typeof globalThis.fetch, token: 'github_pat_secret' }
    const first = await reportUpdateFailure(config(stateDirectory), status, deps)
    const second = await reportUpdateFailure(config(stateDirectory), status, deps)
    expect(first).toContain('/issues/1')
    expect(second).toBeNull()
    expect(fetch).toHaveBeenCalledTimes(1)
    const call = (fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls[0]!
    const payload = JSON.parse(String(call[1]?.body)) as { body: string }
    expect(payload.body).toContain('[REDACTED]')
    expect(payload.body).toContain('[PATH]')
    expect(payload.body).not.toContain('ghp_secret')
    expect(payload.body).not.toContain('A:\\private')
    rmSync(stateDirectory, { recursive: true, force: true })
  })
})
