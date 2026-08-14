import { describe, expect, it, vi } from 'vitest'

import {
  detectToolchain,
  ensureToolchain,
  npmInvocation,
  toolchainEnvironment
} from '../src/main/update/toolchain'
import type { RunOptions, RunResult } from '../src/main/update/run-command'
import { readRemoteHead, syncCheckout, SourceError } from '../src/main/update/source-checkout'

const ENV = {
  ProgramFiles: 'C:\\Program Files',
  'ProgramFiles(x86)': 'C:\\Program Files (x86)',
  USERPROFILE: 'C:\\Users\\eva',
  LOCALAPPDATA: 'C:\\Users\\eva\\AppData\\Local',
  Path: 'C:\\Windows'
} satisfies NodeJS.ProcessEnv

const GIT = 'C:\\Program Files\\Git\\cmd\\git.exe'
const NODE = 'C:\\Program Files\\nodejs\\node.exe'
const CARGO = 'C:\\Users\\eva\\.cargo\\bin\\cargo.exe'
const VSWHERE = 'C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe'

function ok(tail: string[] = [], raw: string[] = []): RunResult {
  return { code: 0, tail, raw, timedOut: false }
}

function fail(): RunResult {
  return { code: 1, tail: ['not found'], raw: [], timedOut: false }
}

describe('toolchain detection', () => {
  it('finds tools by absolute path, so a fresh install needs no new PATH', async () => {
    const installed = new Set([GIT, NODE, CARGO, VSWHERE])
    const run = vi.fn(async (options: RunOptions) =>
      options.file === VSWHERE ? ok(['[PATH]']) : fail()
    )

    const report = await detectToolchain({ env: ENV, exists: (path) => installed.has(path), run })

    expect(report.complete).toBe(true)
    expect(report.tools.map((tool) => tool.path)).toEqual([GIT, NODE, CARGO, VSWHERE])
    // vswhere only reports the linker; it is not a build command itself.
    expect(report.pathEntries).toEqual([
      'C:\\Program Files\\Git\\cmd',
      'C:\\Program Files\\nodejs',
      'C:\\Users\\eva\\.cargo\\bin'
    ])
  })

  it('falls back to a PATH lookup when nothing is at the known location', async () => {
    const run = vi.fn(async (options: RunOptions) => (options.file === 'git' ? ok() : fail()))

    const report = await detectToolchain({ env: ENV, exists: () => false, run })

    expect(report.complete).toBe(false)
    expect(report.tools.find((tool) => tool.id === 'git')?.path).toBe('git')
    expect(report.tools.find((tool) => tool.id === 'rust')?.available).toBe(false)
    // No vswhere means no MSVC, and no probe is attempted for it.
    expect(report.tools.find((tool) => tool.id === 'msvc')?.available).toBe(false)
  })

  it('reports a clear reason when nothing can be installed', async () => {
    const run = vi.fn(async () => fail())

    const { error } = await ensureToolchain({ env: ENV, exists: () => false, run })

    expect(error).toContain('winget')
  })

  it('installs only what is missing and re-checks the result', async () => {
    const installed = new Set([GIT, NODE, VSWHERE])
    const run = vi.fn(async (options: RunOptions) => {
      if (options.file === VSWHERE) return ok(['[PATH]'])
      if (options.file !== 'winget') return fail()
      if (options.args[0] === '--version') return ok(['1.9'])
      // The winget run makes cargo appear where detection looks for it.
      installed.add(CARGO)
      return ok()
    })

    const { report, error } = await ensureToolchain({
      env: ENV,
      exists: (path) => installed.has(path),
      run
    })

    expect(error).toBeNull()
    expect(report.complete).toBe(true)
    const installs = run.mock.calls
      .map(([options]) => options.args)
      .filter((args) => args[0] === 'install')
    expect(installs).toHaveLength(1)
    expect(installs[0]).toContain('Rustup.Rustup')
  })

  it('puts the resolved directories in front of PATH', () => {
    const environment = toolchainEnvironment(
      { complete: true, pathEntries: ['C:\\Program Files\\nodejs', 'C:\\Windows'], tools: [] },
      ENV
    )

    expect(environment['Path']).toBe('C:\\Program Files\\nodejs;C:\\Windows')
    expect(environment['PATH']).toBe(environment['Path'])
  })

  it('runs npm through its JS entry point instead of the .cmd shim', () => {
    const cli = 'C:\\Program Files\\nodejs\\node_modules\\npm\\bin\\npm-cli.js'
    const report = {
      complete: true,
      pathEntries: [],
      tools: [{ id: 'node' as const, label: 'Node.js 22', available: true, path: NODE }]
    }

    expect(npmInvocation(report, (path) => path === cli)).toEqual({ file: NODE, args: [cli] })
    expect(npmInvocation(report, () => false)).toBeNull()
  })
})

describe('source checkout', () => {
  const options = {
    directory: 'C:\\data\\EvoHime\\source',
    repositoryUrl: 'https://example.invalid/evo.git',
    branch: 'main'
  }
  const head = 'c'.repeat(40)

  it('reads the branch tip without a local checkout', async () => {
    const run = vi.fn(async (_call: RunOptions) => ok([], [`${head}\trefs/heads/main`]))

    await expect(readRemoteHead(options, { git: GIT, run, exists: () => false })).resolves.toBe(head)
    expect(run.mock.calls[0]?.[0]?.args).toEqual([
      'ls-remote',
      options.repositoryUrl,
      'refs/heads/main'
    ])
  })

  it('refuses a branch the remote does not have', async () => {
    const run = vi.fn(async () => ok([], []))

    await expect(readRemoteHead(options, { git: GIT, run, exists: () => false })).rejects.toBeInstanceOf(
      SourceError
    )
  })

  it('resets an existing checkout to the wanted commit and keeps build caches', async () => {
    const run = vi.fn(async (call: RunOptions) =>
      call.args[0] === 'rev-parse' ? ok([], [head]) : ok()
    )

    await expect(syncCheckout(options, head, { git: GIT, run, exists: () => true })).resolves.toBe(head)

    const commands = run.mock.calls.map(([call]) => call.args)
    expect(commands.some((args) => args[0] === 'clone')).toBe(false)
    expect(commands).toContainEqual(['fetch', '--prune', 'origin', 'main'])
    expect(commands).toContainEqual(['reset', '--hard', head])
    const clean = commands.find((args) => args[0] === 'clean')
    expect(clean).toContain('target')
    expect(clean).toContain('desktop/evohime-electron/node_modules')
  })

  it('clones when there is no checkout yet', async () => {
    const run = vi.fn(async (call: RunOptions) =>
      call.args[0] === 'rev-parse' ? ok([], [head]) : ok()
    )
    let cloned = false

    await syncCheckout(options, head, {
      git: GIT,
      run,
      exists: () => {
        const seen = cloned
        cloned = true
        return seen
      }
    })

    expect(run.mock.calls[0]?.[0].args[0]).toBe('clone')
  })

  it('refuses a commit that is not an object name', async () => {
    const run = vi.fn(async () => ok())

    await expect(
      syncCheckout(options, 'HEAD; rm -rf /', { git: GIT, run, exists: () => true })
    ).rejects.toBeInstanceOf(SourceError)
    expect(run).not.toHaveBeenCalled()
  })
})
