import { spawn, type ChildProcess } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import type { CoreEvent, ShellState } from '../../src/shared/api'
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
const CORE_STARTUP_TIMEOUT_MS = 60_000

let core: ChildProcess | null = null
let client: CorePipeClient | null = null
const dataDirs: string[] = []

afterEach(async () => {
  client?.stop()
  client = null
  const processToStop = core
  core = null
  if (processToStop && processToStop.exitCode === null && processToStop.signalCode === null) {
    await new Promise<void>((resolvePromise) => {
      const finish = (): void => {
        processToStop.removeListener('exit', finish)
        processToStop.removeListener('error', finish)
        resolvePromise()
      }
      processToStop.once('exit', finish)
      processToStop.once('error', finish)
      processToStop.kill()
    })
  }
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
      EVOHIME_DEV_MODE: '1',
      EVOHIME_LAUNCH_CONTEXT: undefined,
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
      clientRole: 'shell',
      secret: '',
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
  }, 90_000)

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
  }, 90_000)

  /**
   * Один шаблон, запущенный против настоящего Core (план 06.3).
   *
   * Проверяется весь путь: каталог шаблонов, запуск с bounded входом,
   * durable-проекция запуска и его события. Никакого внешнего web runtime при
   * этом не поднимается — работает только уже собранный `evohime-core.exe`.
   */
  it('runs one Core-owned workflow template end to end', async () => {
    const pipeName = `\\\\.\\pipe\\evohime-e2e-${process.pid}-workflow`
    startCore(pipeName)

    const target = createClient(pipeName)
    const events: CoreEvent[] = []
    target.on('core-event', (event) => events.push(event))
    target.start()
    await waitForState(target, 'connected', (state) => state.connection === 'connected')
    const awaitEvent = async (eventType: string, retry?: () => void): Promise<CoreEvent> => {
      const deadline = Date.now() + 30_000
      let nextRetryAt = Date.now() + 1_000
      for (;;) {
        const found = events.filter((event) => event.eventType === eventType).at(-1)
        if (found) return found
        if (Date.now() > deadline) throw new Error(`timed out waiting for ${eventType}`)
        if (retry && Date.now() >= nextRetryAt) {
          retry()
          nextRetryAt = Date.now() + 1_000
        }
        await new Promise((resolvePromise) => setTimeout(resolvePromise, 100))
      }
    }

    const listTemplates = (): void => {
      target.send({ listWorkflowTemplates: {} })
    }
    listTemplates()
    const catalog = JSON.parse((await awaitEvent('workflow.templates', listTemplates)).payload) as {
      templates: { template_id: string }[]
    }
    expect(catalog.templates.map((item) => item.template_id)).toContain('parallel-security-review')

    target.send({
      startWorkflow: {
        templateId: 'parallel-security-review',
        taskId: 'e2e-task',
        workspacePath: repoRoot,
        inputs: [{ name: 'scope', value: 'crates' }],
        idempotencyKey: `e2e-${process.pid}`
      }
    })
    const started = JSON.parse((await awaitEvent('workflow.started')).payload) as {
      run_id: string
      error_code: string
    }
    expect(started.error_code).toBe('')
    expect(started.run_id).toBeTruthy()

    // Запуск идёт в ядре: оболочка только спрашивает состояние.
    let projection: { state: string; nodes: { node_id: string }[] } | null = null
    const deadline = Date.now() + 60_000
    while (Date.now() < deadline) {
      target.send({ getWorkflowRun: { runId: started.run_id } })
      const payload = JSON.parse((await awaitEvent('workflow.run')).payload) as {
        run_id: string
        state: string
        nodes: { node_id: string }[]
      }
      if (payload.run_id === started.run_id) {
        projection = payload
        if (['completed', 'failed', 'degraded', 'cancelled'].includes(payload.state)) break
      }
      await new Promise((resolvePromise) => setTimeout(resolvePromise, 300))
    }
    expect(projection).not.toBeNull()
    expect(projection?.nodes.map((node) => node.node_id).sort()).toEqual([
      'merge',
      'permissions',
      'scope',
      'secrets'
    ])

    target.send({
      listWorkflowEvents: { runId: started.run_id, afterSequence: -1, limit: 100 }
    })
    const eventList = JSON.parse((await awaitEvent('workflow.events')).payload) as {
      events: { sequence: number; event_type: string }[]
    }
    expect(eventList.events[0]?.event_type).toBe('workflow.run_started')
    const sequences = eventList.events.map((item) => item.sequence)
    expect(sequences).toEqual([...sequences].sort((left, right) => left - right))
  }, 120_000)
})
