import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterAll, describe, expect, it } from 'vitest'

import { buildStagedPackage } from '../../src/main/update/builder'
import { readBuildMarker } from '../../src/main/update/config'
import { readRemoteHead, syncCheckout } from '../../src/main/update/source-checkout'
import { detectToolchain, toolPath } from '../../src/main/update/toolchain'

const REPOSITORY = 'https://github.com/rkfsociety/EvoHime.git'

/**
 * End-to-end check of the source update against the real toolchain.
 *
 * Unit tests cover the orchestration with stubs; this one proves the parts that
 * only fail for real: cargo and npm actually run, the staged package has the
 * layout the installer produces, and `evohime-transaction.exe` swaps it into an
 * installation directory.
 *
 * It rebuilds the product, so it is opt-in and slow — a cold clone with no cargo
 * or npm cache takes tens of minutes. The update is run exactly as in
 * production: a clone of the tracked branch in its own directory, never this
 * checkout (a rebuild reinstalls `node_modules`, which would pull the test
 * runner out from under itself) and never an installed client:
 *   $env:EVOHIME_UPDATE_E2E = '1'; npx vitest run tests/e2e/source-update.e2e.test.ts
 */

const enabled = process.env['EVOHIME_UPDATE_E2E'] === '1'
const repoRoot = resolve(__dirname, '..', '..', '..', '..')
const roots: string[] = []

afterAll(() => {
  for (const root of roots.splice(0, roots.length)) {
    try {
      rmSync(root, { recursive: true, force: true })
    } catch {
      // A build directory can stay locked briefly; leftovers are temporary.
    }
  }
})

function temporaryRoot(name: string): string {
  const root = mkdtempSync(join(tmpdir(), `evohime-update-e2e-${name}-`))
  roots.push(root)
  return root
}

describe.skipIf(!enabled)('source update end to end', () => {
  it('reads the tracked branch tip from the real remote', async () => {
    const toolchain = await detectToolchain({})
    const git = toolPath(toolchain, 'git')
    expect(git, 'git is required for a source update').not.toBeNull()

    const head = await readRemoteHead(
      { directory: repoRoot, repositoryUrl: REPOSITORY, branch: 'main' },
      { git: git as string }
    )

    expect(head).toMatch(/^[0-9a-f]{40}$/)
  })

  it('clones, rebuilds and swaps the package into an installation', async () => {
    const toolchain = await detectToolchain({})
    expect(toolchain.complete, 'the full build toolchain is required').toBe(true)
    const git = toolPath(toolchain, 'git') as string

    const checkout = join(temporaryRoot('source'), 'source')
    const staging = join(temporaryRoot('staging'), 'staging')
    const options = { directory: checkout, repositoryUrl: REPOSITORY, branch: 'main' }

    const commit = await readRemoteHead(options, { git })
    await expect(syncCheckout(options, commit, { git })).resolves.toBe(commit)

    const lines: string[] = []
    const marker = await buildStagedPackage(
      { sourceDirectory: checkout, stagingDirectory: staging, commit, branch: 'main', toolchain },
      { onLine: (line) => lines.push(line) }
    )

    expect(marker.commit).toBe(commit)
    for (const component of [
      'EvoHime.exe',
      'evohime-core.exe',
      'evohime-supervisor.exe',
      'evohime-transaction.exe',
      'evohime.manifest.json',
      'evohime.build.json',
      join('resources', 'app.asar')
    ]) {
      expect(existsSync(join(staging, component)), `staged ${component}`).toBe(true)
    }
    expect(readBuildMarker(staging)?.commit).toBe(commit)

    // The swap runs against a throwaway installation, never the real one.
    const root = temporaryRoot('install')
    const install = join(root, 'install')
    const state = join(root, 'state')
    mkdirSync(install, { recursive: true })
    for (const component of [
      'EvoHime.exe',
      'evohime-core.exe',
      'evohime-supervisor.exe',
      'evohime.manifest.json'
    ]) {
      writeFileSync(join(install, component), `previous:${component}`)
    }
    writeFileSync(join(install, 'user-data.txt'), 'keep')

    execFileSync(join(staging, 'evohime-transaction.exe'), [
      '--worker',
      '--apply-staging',
      '--staging',
      staging,
      '--install-dir',
      install,
      '--state-dir',
      state
    ])

    expect(readBuildMarker(install)?.commit).toBe(commit)
    expect(existsSync(join(install, 'resources', 'app.asar'))).toBe(true)
    // Local files survive; the transaction journal is gone after the commit.
    expect(readFileSync(join(install, 'user-data.txt'), 'utf8')).toBe('keep')
    expect(existsSync(join(state, 'transaction.json'))).toBe(false)
  }, 90 * 60_000)
})
