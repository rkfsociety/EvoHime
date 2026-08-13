import { describe, expect, it } from 'vitest'

import { resolveIdentity, resolveRepository } from '../src/main/identity'

/**
 * The account row names the user from the tools they already configured, and
 * the project row reads git state of the open workspace. Both run fixed
 * executables with fixed arguments; any failure falls back instead of
 * surfacing an error.
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
    const { run } = probes({ 'gh api user --jq .login': '  weird\nsecond line\n' })

    expect(await resolveIdentity(run)).toEqual({ name: 'weird', source: 'github' })
  })
})

describe('repository summary', () => {
  function gitProbes(answers: Record<string, string | null>) {
    const cwds: string[] = []
    const run = async (
      file: string,
      args: readonly string[],
      cwd: string
    ): Promise<string | null> => {
      cwds.push(cwd)
      return answers[[file, ...args].join(' ')] ?? null
    }
    return { run, cwds }
  }

  it('reports the branch and sums uncommitted lines', async () => {
    const { run, cwds } = gitProbes({
      'git rev-parse --abbrev-ref HEAD': 'main\n',
      // A binary file reports "-" instead of counts and must not break the sum.
      'git diff --numstat HEAD': '12\t3\tsrc/a.ts\n5\t0\tsrc/b.ts\n-\t-\tlogo.png\n'
    })

    expect(await resolveRepository('C:\\work\\repo', run)).toEqual({
      branch: 'main',
      added: 17,
      removed: 3
    })
    // git runs inside the workspace, never in the shell's own directory.
    expect(new Set(cwds)).toEqual(new Set(['C:\\work\\repo']))
  })

  it('reports nothing for a folder outside git', async () => {
    const { run } = gitProbes({})

    expect(await resolveRepository('C:\\work\\plain', run)).toBeNull()
  })

  it('refuses a relative path instead of running git somewhere else', async () => {
    const { run, cwds } = gitProbes({ 'git rev-parse --abbrev-ref HEAD': 'main' })

    expect(await resolveRepository('..\\elsewhere', run)).toBeNull()
    expect(cwds).toEqual([])
  })
})
