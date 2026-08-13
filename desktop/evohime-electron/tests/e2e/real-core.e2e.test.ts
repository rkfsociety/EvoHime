import { spawn, type ChildProcess } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import type { ShellState } from '../../src/shared/api'
import type { LaunchContext } from '../../src/main/ipc/launch-context'
import { CorePipeClient } from '../../src/main/ipc/pipe-client'

/**
 * End-to-end IPC checks against a real, built `evohime-core.exe`.
 *
 * Skipped when no Core binary is present, so a checkout without a Rust build
 * still runs the rest of the suite. Build one with:
 *   cargo build -p evohime-core --release
 */

const repoRoot = resolve(__dirname, '..', '..', '..', '..')

function findCoreExecutable(): string | null {
  const explicit = process.env['EVOHIME_CORE_EXE']?.trim()
  const candidates = [
    ...(explicit ? [explicit] : []),
    join(repoRoot, 'target', 'release', 'evohime-core.exe'),
    join(repoRoot, 'target', 'debug', 'evohime-core.exe')
  ]
  return candidates.find((candidate) => existsSync(candidate)) ?? null
}

const coreExecutable = findCoreExecutable()

let core: ChildProcess | null = null
let client: CorePipeClient | null = null
const dataDirs: string[] = []

afterEach(() => {
  client?.stop()
  client = null
  core?.kill()
  core = null
  for (const directory of dataDirs.splice(0, dataDirs.length)) {
    try {
      rmSync(directory, { recursive: true, force: true })
    } catch {
      // Core may still be releasing its SQLite handle; the OS temp directory
      // is cleaned up by the platform, and a leftover file is not a failure.
    }
  }
})

/**
 * Each Core instance gets its own data directory so a restart never races the
 * previous process for the same SQLite file.
 */
function startCore(pipeName: string): ChildProcess {
  const dataDir = mkdtempSync(join(tmpdir(), 'evohime-e2e-'))
  dataDirs.push(dataDir)
  const child = spawn(coreExecutable as string, {
    env: {
      ...process.env,
      EVOHIME_CORE_PIPE: pipeName,
      EVOHIME_DATA_DIR: dataDir
    },
    stdio: 'ignore',
    windowsHide: true
  })
  core = child
  return child
}

function createClient(pipeName: string): CorePipeClient {
  const created = new CorePipeClient({
    launch: {
      pipeName,
      clientId: 'e2e-shell',
      sessionId: 'e2e-session',
      challenge: '',
      livenessEvent: '',
      developerLaunch: true
    } satisfies LaunchContext,
    connectTimeoutMs: 2_000,
    handshakeTimeoutMs: 5_000,
    backoff: { baseMs: 100, maxMs: 500, jitterRatio: 0 }
  })
  client = created
  return created
}

function waitForState(
  target: CorePipeClient,
  label: string,
  predicate: (state: ShellState) => boolean,
  timeoutMs = 20_000
): Promise<ShellState> {
  return new Promise((resolvePromise, reject) => {
    if (predicate(target.state)) {
      resolvePromise(target.state)
      return
    }
    const timer = setTimeout(
      () => reject(new Error(`timed out waiting for ${label}, last=${target.state.connection}`)),
      timeoutMs
    )
    const listener = (state: ShellState): void => {
      if (predicate(state)) {
        clearTimeout(timer)
        target.off('state', listener)
        resolvePromise(state)
      }
    }
    target.on('state', listener)
  })
}

describe.runIf(coreExecutable !== null && process.platform === 'win32')('real Core IPC', () => {
  it('handshakes with a freshly started Core', async () => {
    const pipeName = `\\\\.\\pipe\\evohime-e2e-${process.pid}-handshake`
    startCore(pipeName)

    const target = createClient(pipeName)
    const connected = waitForState(target, 'connected', (state) => state.connection === 'connected')
    target.start()
    const state = await connected

    expect(state.protocol).toEqual({ major: 1, minor: 0 })
    expect(state.coreVersion).toBeTruthy()
  }, 30_000)

  it('reconnects after Core is killed and restarted', async () => {
    const pipeName = `\\\\.\\pipe\\evohime-e2e-${process.pid}-restart`
    startCore(pipeName)

    const target = createClient(pipeName)
    target.start()
    await waitForState(target, 'first connect', (state) => state.connection === 'connected')

    core?.kill()
    await waitForState(target, 'reconnecting', (state) => state.connection === 'reconnecting')

    startCore(pipeName)
    const recovered = await waitForState(target, 'reconnected', (state) => state.connection === 'connected')
    expect(recovered.protocol).toEqual({ major: 1, minor: 0 })
  }, 40_000)
})
