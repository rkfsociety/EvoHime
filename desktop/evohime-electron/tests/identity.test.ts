import { describe, expect, it } from 'vitest'

import { resolveIdentity } from '../src/main/identity'

/**
 * The account row names the user from the tools they already configured. The
 * probes run fixed executables with fixed arguments, and any failure simply
 * moves on to the next source instead of surfacing an error.
 */

function probes(answers: Record<string, string | null>) {
  const seen: string[] = []
  const run = async (file: string, args: readonly string[]): Promise<string | null> => {
    const key = [file, ...args].join(' ')
    seen.push(key)
    return answers[key] ?? null
  }
  return { run, seen }
}

describe('user identity', () => {
  it('prefers the GitHub login', async () => {
    const { run, seen } = probes({ 'gh api user --jq .login': 'rkfsociety\n' })

    expect(await resolveIdentity(run)).toEqual({ name: 'rkfsociety', source: 'github' })
    // git is not consulted once GitHub answered.
    expect(seen).toEqual(['gh api user --jq .login'])
  })

  it('falls back to git when the GitHub CLI is absent', async () => {
    const { run } = probes({ 'git config --get user.name': 'Roman K\n' })

    expect(await resolveIdentity(run)).toEqual({ name: 'Roman K', source: 'git' })
  })

  it('falls back to the OS account when neither tool answers', async () => {
    const { run } = probes({})

    const identity = await resolveIdentity(run)
    expect(identity.source).toBe('os')
    expect(identity.name.length).toBeGreaterThan(0)
  })

  it('bounds and cleans what a tool printed', async () => {
    const { run } = probes({ 'gh api user --jq .login': `  weird\nsecond line\n` })

    expect(await resolveIdentity(run)).toEqual({ name: 'weird', source: 'github' })
  })
})
