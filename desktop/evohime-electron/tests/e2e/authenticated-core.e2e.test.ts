import { execFileSync, spawn, type ChildProcess } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import type { ShellState } from '../../src/shared/api'
import type { LaunchContext } from '../../src/main/ipc/launch-context'
import { CorePipeClient } from '../../src/main/ipc/pipe-client'

/**
 * End-to-end checks of the enforced handshake against a real Core started with
 * a supervisor-style launch context: the pipe carries an owner-only DACL, Core
 * issues a single-use nonce, and only a client that proves knowledge of the
 * session secret is served.
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
const CORE_STARTUP_TIMEOUT_MS = 60_000

/** SID of the account running the tests, as Core will observe it. */
function currentUserSid(): string {
  // The absolute path avoids a coreutils `whoami` shadowing the Windows one.
  const whoami = join(process.env['SystemRoot'] ?? 'C:\\Windows', 'System32', 'whoami.exe')
  const output = execFileSync(whoami, ['/user', '/fo', 'csv', '/nh'], { encoding: 'utf8' })
  const sid = output.split(',').pop()?.trim().replace(/"/g, '') ?? ''
  return sid
}

let core: ChildProcess | null = null
let client: CorePipeClient | null = null
const directories: string[] = []

afterEach(() => {
  client?.stop()
  client = null
  core?.kill()
  core = null
  for (const directory of directories.splice(0, directories.length)) {
    try {
      rmSync(directory, { recursive: true, force: true })
    } catch {
      // Core may still hold its SQLite file; the OS cleans the temp directory.
    }
  }
})

interface StartedCore {
  readonly pipeName: string
  readonly secret: string
}

function startAuthenticatedCore(suffix: string, userSid: string): StartedCore {
  const dataDir = mkdtempSync(join(tmpdir(), 'evohime-auth-'))
  directories.push(dataDir)

  const pipeName = `\\\\.\\pipe\\evohime-auth-${process.pid}-${suffix}`
  const secret = 'ab'.repeat(32)
  const contextPath = join(dataDir, 'session.json')
  writeFileSync(
    contextPath,
    JSON.stringify({
      pipe_name: pipeName,
      secret,
      expected_user_sid: userSid,
      expected_logon_session: '',
      issued_at_ms: Date.now()
    }),
    'utf8'
  )

  core = spawn(coreExecutable as string, {
    env: { ...process.env, EVOHIME_LAUNCH_CONTEXT: contextPath, EVOHIME_DATA_DIR: dataDir },
    stdio: 'ignore',
    windowsHide: true
  })
  return { pipeName, secret }
}

function createClient(pipeName: string, secret: string, clientRole = 'shell'): CorePipeClient {
  const created = new CorePipeClient({
    launch: {
      pipeName,
      clientId: 'e2e-shell',
      sessionId: 'e2e-session',
      clientRole,
      secret,
      livenessEvent: '',
      developerLaunch: false
    } satisfies LaunchContext,
    connectTimeoutMs: 2_000,
    handshakeTimeoutMs: 5_000,
    backoff: { baseMs: 100, maxMs: 400, jitterRatio: 0 }
  })
  client = created
  return created
}

function waitForState(
  target: CorePipeClient,
  label: string,
  predicate: (state: ShellState) => boolean,
  timeoutMs = CORE_STARTUP_TIMEOUT_MS
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

describe.runIf(coreExecutable !== null && process.platform === 'win32')(
  'authenticated Core IPC',
  () => {
    it('serves a shell that proves knowledge of the session secret', async () => {
      const sid = currentUserSid()
      const started = startAuthenticatedCore('valid', sid)

      const target = createClient(started.pipeName, started.secret)
      const connected = waitForState(
        target,
        'connected',
        (state) => state.connection === 'connected'
      )
      target.start()

      expect((await connected).protocol).toEqual({ major: 1, minor: 0 })
    }, 90_000)

    it('accepts the WinUI compatibility role while the fallback is supported', async () => {
      const started = startAuthenticatedCore('compat', currentUserSid())

      const target = createClient(started.pipeName, started.secret, 'compatibility-shell')
      const connected = waitForState(
        target,
        'connected',
        (state) => state.connection === 'connected'
      )
      target.start()

      expect((await connected).coreVersion).toBeTruthy()
    }, 90_000)

    it('refuses an unknown client role', async () => {
      const started = startAuthenticatedCore('role', currentUserSid())

      const target = createClient(started.pipeName, started.secret, 'diagnostics')
      const fatal = waitForState(target, 'fatal', (state) => state.connection === 'fatal')
      target.start()

      expect((await fatal).reason).toBe('auth-rejected')
    }, 90_000)

    it('refuses a shell with the wrong secret and stops retrying', async () => {
      const sid = currentUserSid()
      const started = startAuthenticatedCore('forged', sid)

      const target = createClient(started.pipeName, 'cd'.repeat(32))
      const fatal = waitForState(target, 'fatal', (state) => state.connection === 'fatal')
      target.start()

      expect((await fatal).reason).toBe('auth-rejected')
    }, 90_000)
  }
)
