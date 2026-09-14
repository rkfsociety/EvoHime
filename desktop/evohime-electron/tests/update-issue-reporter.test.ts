import { describe, expect, it, vi } from 'vitest'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { reportSupportBundle, reportUpdateFailure } from '../src/main/update/update-issue-reporter'
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

  it('embeds a bounded support archive in a user-triggered issue', async () => {
    const stateDirectory = mkdtempSync(join(tmpdir(), 'evohime-support-issue-'))
    const fetch = vi.fn(async () => new Response(JSON.stringify({ html_url: 'https://github.com/rkfsociety/EvoHime/issues/2' }), { status: 201, headers: { 'content-type': 'application/json' } }))
    const archive = Uint8Array.from([0x50, 0x4b, 0x03, 0x04, 0x01, 0x02])
    const url = await reportSupportBundle(config(stateDirectory), { archive, issueDraft: '### Problem\nСбой без секретов.' }, { fetch: fetch as typeof globalThis.fetch, token: 'test-token-value-1234567890' })
    expect(url).toContain('/issues/2')
    const call = (fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls[0]!
    const payload = JSON.parse(String(call[1]?.body)) as { body: string }
    expect(payload.body).toContain(Buffer.from(archive).toString('base64'))
    expect(payload.body).toContain('SHA-256')
    rmSync(stateDirectory, { recursive: true, force: true })
  })

  it('refuses an archive that cannot fit in an issue', async () => {
    const stateDirectory = mkdtempSync(join(tmpdir(), 'evohime-support-issue-large-'))
    await expect(reportSupportBundle(config(stateDirectory), { archive: new Uint8Array(40 * 1024 + 1), issueDraft: '' }, { token: 'test-token-value-1234567890' })).rejects.toThrow('слишком большой')
    rmSync(stateDirectory, { recursive: true, force: true })
  })
})
