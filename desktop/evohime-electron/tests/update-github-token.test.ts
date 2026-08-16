import { describe, expect, it, vi } from 'vitest'

import type { RunOptions, RunResult } from '../src/main/update/run-command'
import { normalizeGithubToken, resolveGithubToken } from '../src/main/update/github-token'

/**
 * Without a token the update check shares a 60-per-hour budget with everything
 * else behind the same address, and an exhausted budget stops updates with a
 * bare `403`. These tests pin where the credential may come from — and that a
 * missing one is never an error.
 */

const TOKEN = 'ghp_0123456789abcdefghijklmnopqrstuvwx'
const OTHER = 'github_pat_0123456789abcdefghijklmnop'

function runner(result: Partial<RunResult>): (options: RunOptions) => Promise<RunResult> {
  return vi.fn(async () => ({ code: 0, tail: [], raw: [], timedOut: false, ...result }))
}

describe('token shape', () => {
  it('accepts the alphabets GitHub issues', () => {
    expect(normalizeGithubToken(TOKEN)).toBe(TOKEN)
    expect(normalizeGithubToken(` ${OTHER}\n`)).toBe(OTHER)
    expect(normalizeGithubToken('a'.repeat(40))).toBe('a'.repeat(40))
  })

  it('rejects anything that cannot go into a header as is', () => {
    for (const value of ['', 'short', `${TOKEN} x`, `${TOKEN}\r\nx: y`, 'токен-кириллицей-длинный', 42, null]) {
      expect(normalizeGithubToken(value), String(value)).toBeNull()
    }
  })
})

describe('where the credential comes from', () => {
  it('prefers the dedicated variable over everything else', async () => {
    const run = runner({ raw: [OTHER] })

    await expect(
      resolveGithubToken({
        environment: { EVOHIME_UPDATE_GITHUB_TOKEN: TOKEN, GITHUB_TOKEN: OTHER },
        configured: OTHER,
        run
      })
    ).resolves.toEqual({ token: TOKEN, source: 'environment' })
    expect(run).not.toHaveBeenCalled()
  })

  it('falls back to update.json, then to an ambient token', async () => {
    await expect(
      resolveGithubToken({ environment: { GITHUB_TOKEN: OTHER }, configured: TOKEN, run: runner({}) })
    ).resolves.toEqual({ token: TOKEN, source: 'config' })

    await expect(
      resolveGithubToken({ environment: { GH_TOKEN: TOKEN }, run: runner({}) })
    ).resolves.toEqual({ token: TOKEN, source: 'environment' })
  })

  it('asks the gh CLI when nothing else has a token', async () => {
    const run = runner({ raw: [TOKEN] })

    await expect(resolveGithubToken({ environment: {}, run })).resolves.toEqual({
      token: TOKEN,
      source: 'gh-cli'
    })
    expect(run).toHaveBeenCalledWith(expect.objectContaining({ file: 'gh', args: ['auth', 'token'] }))
  })

  it('ignores a malformed value from the config file', async () => {
    // Иначе строка из файла ушла бы в заголовок запроса как есть.
    await expect(
      resolveGithubToken({ environment: {}, configured: 'not a token', run: runner({}) })
    ).resolves.toBeNull()
  })

  it('is simply absent when gh is missing, signed out or hung', async () => {
    for (const result of [{ code: 1, raw: [TOKEN] }, { code: -1, timedOut: true }, { raw: ['not a token'] }]) {
      await expect(resolveGithubToken({ environment: {}, run: runner(result) })).resolves.toBeNull()
    }

    const throwing = vi.fn(async () => {
      throw new Error('spawn ENOENT')
    })
    await expect(resolveGithubToken({ environment: {}, run: throwing })).resolves.toBeNull()
  })
})
