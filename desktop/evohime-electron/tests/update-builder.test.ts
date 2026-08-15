import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import {
  assembleStaging,
  buildStagedPackage,
  DEPENDENCY_MARKER,
  ELECTRON_SUBPATH,
  REQUIRED_NATIVE_COMPONENTS
} from '../src/main/update/builder'
import type { RunOptions, RunResult } from '../src/main/update/run-command'
import type { ToolchainReport } from '../src/main/update/toolchain'

/**
 * Cost of a rebuild, not its correctness: a local update runs on the user's
 * machine while they wait, so the steps that change nothing must be skipped.
 */

const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0, roots.length)) {
    rmSync(root, { recursive: true, force: true })
  }
})

function temporaryRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'evohime-builder-test-'))
  roots.push(root)
  return root
}

function write(path: string, content: string): void {
  mkdirSync(join(path, '..'), { recursive: true })
  writeFileSync(path, content)
}

const toolchain: ToolchainReport = {
  complete: true,
  pathEntries: [],
  tools: [
    { id: 'git', label: 'git', available: true, path: 'C:\\tools\\git.exe' },
    { id: 'node', label: 'node', available: true, path: 'C:\\tools\\node.exe' },
    { id: 'rust', label: 'rust', available: true, path: 'C:\\tools\\cargo.exe' },
    { id: 'msvc', label: 'msvc', available: true, path: 'C:\\tools\\link.exe' }
  ]
}

describe('staging assembly', () => {
  /** Sources, a packaged tree and the staging directory the swap reads. */
  function scenario(): { source: string; unpacked: string; staging: string } {
    const root = temporaryRoot()
    const source = join(root, 'source')
    const unpacked = join(root, 'unpacked')
    const staging = join(root, 'staging')

    for (const component of REQUIRED_NATIVE_COMPONENTS) {
      write(join(source, 'target', 'release', component), `native:${component}`)
    }
    write(join(unpacked, 'EvoHime.exe'), 'shell')
    write(join(unpacked, 'icudtl.dat'), 'runtime-data-that-never-changes')
    write(join(unpacked, 'resources', 'app.asar'), 'payload')
    return { source, unpacked, staging }
  }

  async function assemble(paths: ReturnType<typeof scenario>): Promise<void> {
    await assembleStaging(
      {
        sourceDirectory: paths.source,
        stagingDirectory: paths.staging,
        installDirectory: join(paths.source, '..', 'install'),
        commit: 'a'.repeat(40),
        branch: 'main',
        toolchain
      },
      join(paths.source, ELECTRON_SUBPATH),
      { exists: (path) => existsOnDisk(path) },
      paths.unpacked
    )
  }

  function existsOnDisk(path: string): boolean {
    try {
      statSync(path)
      return true
    } catch {
      return false
    }
  }

  it('leaves an unchanged runtime file untouched on a second build', async () => {
    const paths = scenario()
    await assemble(paths)

    // Same size and timestamp, different bytes: only a copy would restore them.
    const staged = join(paths.staging, 'icudtl.dat')
    const stamp = statSync(staged)
    writeFileSync(staged, 'RUNTIME-DATA-THAT-NEVER-CHANGES')
    utimesSync(staged, stamp.atime, stamp.mtime)

    await assemble(paths)

    expect(readFileSync(staged, 'utf8')).toBe('RUNTIME-DATA-THAT-NEVER-CHANGES')
  })

  it('copies a file the build regenerated', async () => {
    const paths = scenario()
    await assemble(paths)
    writeFileSync(join(paths.unpacked, 'resources', 'app.asar'), 'payload-v2')

    await assemble(paths)

    expect(readFileSync(join(paths.staging, 'resources', 'app.asar'), 'utf8')).toBe('payload-v2')
  })

  it('drops a file the package no longer contains', async () => {
    const paths = scenario()
    await assemble(paths)
    write(join(paths.staging, 'obsolete.dll'), 'from an older package')

    await assemble(paths)

    expect(existsOnDisk(join(paths.staging, 'obsolete.dll'))).toBe(false)
    expect(existsOnDisk(join(paths.staging, 'evohime.build.json'))).toBe(true)
  })
})

describe('dependency installation', () => {
  /** Records every command a build runs, and pretends all of them succeed. */
  function recorder(): { commands: string[]; run: (options: RunOptions) => Promise<RunResult> } {
    const commands: string[] = []
    return {
      commands,
      run: async (options) => {
        const command = options.args.join(' ')
        commands.push(command)
        // A real `npm ci` creates node_modules; the marker lives inside it.
        if (command.includes('ci')) {
          mkdirSync(join(options.cwd ?? '.', 'node_modules'), { recursive: true })
        }
        return { code: 0, tail: [], raw: [], timedOut: false }
      }
    }
  }

  async function build(source: string, run: (options: RunOptions) => Promise<RunResult>): Promise<void> {
    const electronRoot = join(source, ELECTRON_SUBPATH)
    await buildStagedPackage(
      {
        sourceDirectory: source,
        stagingDirectory: join(source, '..', 'staging'),
        installDirectory: join(source, '..', 'install'),
        commit: 'b'.repeat(40),
        branch: 'main',
        toolchain
      },
      {
        run,
        // The packaging step is stubbed out, so nothing reaches the real
        // assembly; the assertions are about which commands ran.
        exists: (path) => path.endsWith('npm-cli.js') || path.startsWith(electronRoot)
      }
    ).catch(() => undefined)
  }

  it('reinstalls when the lockfile changed and skips when it did not', async () => {
    const source = join(temporaryRoot(), 'source')
    const electronRoot = join(source, ELECTRON_SUBPATH)
    write(join(electronRoot, 'package-lock.json'), '{"packages":{}}')

    const first = recorder()
    await build(source, first.run)
    expect(first.commands.some((command) => command.includes('ci'))).toBe(true)
    // The marker is written by the build; simulate the install that produced it.
    expect(existsOnDisk(join(electronRoot, DEPENDENCY_MARKER))).toBe(true)

    const second = recorder()
    await build(source, second.run)
    expect(second.commands.some((command) => command.includes('ci'))).toBe(false)

    writeFileSync(join(electronRoot, 'package-lock.json'), '{"packages":{"new":{}}}')
    const third = recorder()
    await build(source, third.run)
    expect(third.commands.some((command) => command.includes('ci'))).toBe(true)
  })

  function existsOnDisk(path: string): boolean {
    try {
      statSync(path)
      return true
    } catch {
      return false
    }
  }
})
