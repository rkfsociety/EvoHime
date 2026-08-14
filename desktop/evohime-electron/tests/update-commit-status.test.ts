import { describe, expect, it, vi } from 'vitest'

import {
  githubApiBase,
  listRecentCommits,
  readCommitState,
  selectGreenCommit,
  touchesProductCode,
  isProductPath
} from '../src/main/update/commit-status'

/**
 * The client rebuilds whatever commit it moves to, so a red commit would be
 * compiled and installed on the user's machine. These tests pin the verdict
 * logic, including the cases where the answer must be "do not update".
 */

const API = 'https://api.github.com/repos/rkfsociety/EvoHime'
const TIP = 'a'.repeat(40)
const PREVIOUS = 'b'.repeat(40)

function respond(routes: Record<string, unknown>): typeof globalThis.fetch {
  return vi.fn(async (input: Parameters<typeof globalThis.fetch>[0]) => {
    const url = String(input)
    const key = Object.keys(routes).find((route) => url.includes(route))
    if (key === undefined) {
      return { ok: false, status: 404, json: async () => ({}) } as Response
    }
    return { ok: true, status: 200, json: async () => routes[key] } as Response
  }) as unknown as typeof globalThis.fetch
}

const run = (status: string, conclusion: string | null) => ({ status, conclusion })

describe('repository resolution', () => {
  it('accepts a github https remote and rejects anything else', () => {
    expect(githubApiBase('https://github.com/rkfsociety/EvoHime.git')).toBe(API)
    expect(githubApiBase('https://github.com/rkfsociety/EvoHime')).toBe(API)
    // Checks can only be trusted from the host that runs them.
    expect(githubApiBase('https://git.example.invalid/rkfsociety/EvoHime.git')).toBeNull()
    expect(githubApiBase('https://github.com/rkfsociety')).toBeNull()
    expect(githubApiBase('https://github.com/a/b/c')).toBeNull()
    expect(githubApiBase('not a url')).toBeNull()
  })
})

describe('commit verdict', () => {
  it('is green only when every finished check passed', async () => {
    const fetch = respond({
      'check-runs': { check_runs: [run('completed', 'success'), run('completed', 'skipped')] }
    })

    await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('success')
  })

  it('is pending while any check is still running', async () => {
    const fetch = respond({
      'check-runs': { check_runs: [run('completed', 'success'), run('in_progress', null)] }
    })

    await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('pending')
  })

  it('is red when a check failed or timed out', async () => {
    for (const conclusion of ['failure', 'timed_out']) {
      const fetch = respond({
        'check-runs': { check_runs: [run('completed', 'success'), run('completed', conclusion)] }
      })

      await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('failure')
    }
  })

  it('treats a cancelled run as no verdict rather than a failure', async () => {
    // A push that supersedes a running check cancels it. That commit was never
    // verified, so it is not installed — but calling it broken would be a lie.
    const fetch = respond({
      'check-runs': { check_runs: [run('completed', 'success'), run('completed', 'cancelled')] }
    })

    await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('unknown')
  })

  it('falls back to the older commit-status API when no checks ran', async () => {
    const fetch = respond({ 'check-runs': { check_runs: [] }, '/status': { state: 'success' } })

    await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('success')
  })

  it('is unknown — never green — when nothing reported at all', async () => {
    const fetch = respond({ 'check-runs': { check_runs: [] }, '/status': { state: 'unexpected' } })

    await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('unknown')
    await expect(readCommitState(API, 'not-a-commit', { fetch })).resolves.toBe('unknown')
  })

  it('ignores malformed responses instead of trusting them', async () => {
    const fetch = respond({ 'check-runs': { check_runs: 'nonsense' }, '/status': [] })

    await expect(readCommitState(API, TIP, { fetch })).resolves.toBe('unknown')
  })
})

describe('choosing a commit to install', () => {
  it('takes the newest green commit when the tip is still building', async () => {
    const fetch = vi.fn(async (input: Parameters<typeof globalThis.fetch>[0]) => {
      const url = String(input)
      if (url.includes('/commits?')) {
        return { ok: true, status: 200, json: async () => [{ sha: TIP }, { sha: PREVIOUS }] } as Response
      }
      const body = url.includes(TIP)
        ? { check_runs: [run('in_progress', null)] }
        : { check_runs: [run('completed', 'success')] }
      return { ok: true, status: 200, json: async () => body } as Response
    }) as unknown as typeof globalThis.fetch

    const selected = await selectGreenCommit(API, 'main', 10, { fetch })

    expect(selected.commit).toBe(PREVIOUS)
    expect(selected.tipState).toBe('pending')
  })

  it('reports no candidate when nothing in the window is green', async () => {
    const fetch = respond({
      '/commits?': [{ sha: TIP }, { sha: PREVIOUS }],
      'check-runs': { check_runs: [run('completed', 'failure')] }
    })

    const selected = await selectGreenCommit(API, 'main', 10, { fetch })

    expect(selected.commit).toBeNull()
    expect(selected.tipState).toBe('failure')
  })

  it('bounds the history it asks for and drops malformed object names', async () => {
    const fetch = respond({
      '/commits?': [{ sha: TIP }, { sha: 'HEAD' }, { sha: 42 }, {}, { sha: PREVIOUS }]
    })

    const commits = await listRecentCommits(API, 'main', 1_000, { fetch })

    expect(commits).toEqual([TIP, PREVIOUS])
    const requested = String((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]?.[0])
    expect(requested).toContain('per_page=100')
  })

  it('surfaces an API failure instead of calling the commit green', async () => {
    const fetch = respond({})

    await expect(selectGreenCommit(API, 'main', 10, { fetch })).rejects.toThrow('GitHub API')
  })
})

describe('what counts as product code', () => {
  it('treats documentation, plans and CI config as harmless', () => {
    for (const path of [
      'docs/architecture.md',
      'docs/plans/development-plan.md',
      'README.md',
      'AGENTS.md',
      'installer/release-notes.md',
      '.github/workflows/windows.yml',
      '.gitignore'
    ]) {
      expect(isProductPath(path), path).toBe(false)
    }
  })

  it('treats everything else as product code, including unfamiliar paths', () => {
    for (const path of [
      'crates/evohime-core/src/main.rs',
      'desktop/evohime-electron/src/main/index.ts',
      'installer/EvoHime.iss',
      'scripts/build-windows-native.ps1',
      'Cargo.lock',
      'something/new/we/have/not/seen'
    ]) {
      expect(isProductPath(path), path).toBe(true)
    }
  })

  it('skips a rebuild when the range only touched documentation', async () => {
    const fetch = respond({
      '/compare/': { files: [{ filename: 'docs/architecture.md' }, { filename: 'README.md' }] }
    })

    await expect(touchesProductCode(API, TIP, PREVIOUS, { fetch })).resolves.toBe(false)
  })

  it('rebuilds as soon as one file of the range is product code', async () => {
    const fetch = respond({
      '/compare/': {
        files: [{ filename: 'docs/architecture.md' }, { filename: 'crates/evohime-core/src/lib.rs' }]
      }
    })

    await expect(touchesProductCode(API, TIP, PREVIOUS, { fetch })).resolves.toBe(true)
  })

  it('rebuilds rather than guess when the diff proves nothing', async () => {
    // Пустой, обрезанный или нечитаемый ответ — не доказательство.
    const empty = respond({ '/compare/': { files: [] } })
    await expect(touchesProductCode(API, TIP, PREVIOUS, { fetch: empty })).resolves.toBe(true)

    const truncated = respond({
      '/compare/': { files: Array.from({ length: 100 }, () => ({ filename: 'docs/x.md' })) }
    })
    await expect(touchesProductCode(API, TIP, PREVIOUS, { fetch: truncated })).resolves.toBe(true)

    const malformed = respond({ '/compare/': { files: [{}] } })
    await expect(touchesProductCode(API, TIP, PREVIOUS, { fetch: malformed })).resolves.toBe(true)

    await expect(touchesProductCode(API, 'HEAD', PREVIOUS, { fetch: empty })).resolves.toBe(true)
  })

  it('says nothing changed when both commits are the same', async () => {
    const fetch = vi.fn()
    await expect(
      touchesProductCode(API, TIP, TIP, { fetch: fetch as unknown as typeof globalThis.fetch })
    ).resolves.toBe(false)
    expect(fetch).not.toHaveBeenCalled()
  })
})
