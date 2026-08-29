import { createHmac } from 'node:crypto'
import { createServer, type Server, type Socket } from 'node:net'

import { afterEach, describe, expect, it } from 'vitest'

import type { CoreEvent, ShellState } from '../src/shared/api'
import { encodeFrame } from '../src/main/ipc/frame-codec'
import { FrameReader } from '../src/main/ipc/frame-reader'
import { evohime } from '../src/main/ipc/generated/protocol.js'
import type { LaunchContext } from '../src/main/ipc/launch-context'
import { CorePipeClient } from '../src/main/ipc/pipe-client'

/**
 * Contract tests for the named-pipe adapter against a stub Core that speaks the
 * real generated protobuf envelopes. They cover the spike matrix from plan 0,
 * gate 0: handshake, streaming, oversized frames, sequence gaps, disconnect and
 * reconnect.
 */

const { CommandEnvelope, EventEnvelope } = evohime.desktop.v1

const CORE_INSTANCE = 'core-instance-1'
const SESSION_EPOCH = 7

let server: Server | null = null
let client: CorePipeClient | null = null
let pipeIndex = 0

afterEach(async () => {
  client?.stop()
  client = null
  if (server) {
    const closing = server
    server = null
    await new Promise<void>((resolve) => closing.close(() => resolve()))
  }
})

interface StubOptions {
  /** Called for every decoded command; returns frames to write back. */
  readonly onCommand: (
    command: evohime.desktop.v1.CommandEnvelope,
    socket: Socket
  ) => Uint8Array[] | void
  readonly onConnection?: (socket: Socket) => void
  /** Set to false to simulate a Core that never issues a nonce. */
  readonly issueChallenge?: boolean
}

function uniquePipeName(): string {
  pipeIndex += 1
  return `\\\\.\\pipe\\evohime-test-${process.pid}-${pipeIndex}`
}

async function startStubCore(pipeName: string, options: StubOptions): Promise<Server> {
  const stub = createServer((socket) => {
    const reader = new FrameReader()
    options.onConnection?.(socket)
    if (options.issueChallenge !== false) {
      // Core speaks first on every connection.
      socket.write(challengeFrame())
    }
    socket.on('data', (chunk) => {
      for (const frame of reader.push(chunk)) {
        const command = CommandEnvelope.decode(frame)
        const responses = options.onCommand(command, socket) ?? []
        for (const response of responses) {
          socket.write(response)
        }
      }
    })
    socket.on('error', () => {})
  })
  await new Promise<void>((resolve) => stub.listen(pipeName, resolve))
  return stub
}

const TEST_NONCE = 'cd'.repeat(32)

function challengeFrame(nonce = TEST_NONCE, expiresAtMs = 0): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      eventType: 'ipc.challenge',
      authChallenge: { nonce, expiresAtMs }
    }).finish()
  )
}

function rejectedFrame(): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      eventType: 'ipc.rejected'
    }).finish()
  )
}

function readyFrame(minor = 0): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor },
      sequenceId: 0,
      eventType: 'core.ready',
      coreInstanceId: CORE_INSTANCE,
      sessionEpoch: SESSION_EPOCH,
      ready: { protocol: { major: 1, minor }, coreVersion: '0.1.0-test' }
    }).finish()
  )
}

function eventFrame(sequenceId: number, eventType: string, payload = ''): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      sequenceId,
      taskId: 'task-1',
      eventType,
      payload: new TextEncoder().encode(payload),
      coreInstanceId: CORE_INSTANCE,
      sessionEpoch: SESSION_EPOCH
    }).finish()
  )
}

function resyncEndFrame(sequenceId: number, moreAvailable: boolean): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      sequenceId,
      eventType: 'resync.end',
      payload: new TextEncoder().encode(
        JSON.stringify({ more_available: moreAvailable, latest_sequence: sequenceId })
      ),
      coreInstanceId: CORE_INSTANCE,
      sessionEpoch: SESSION_EPOCH
    }).finish()
  )
}

function ledgerEventFrame(
  sequenceId: number,
  eventId: string,
  coreInstanceId = CORE_INSTANCE,
  sessionEpoch = SESSION_EPOCH
): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      sequenceId,
      taskId: 'task-1',
      eventType: 'ledger.tool_call',
      payload: new TextEncoder().encode('{}'),
      coreInstanceId,
      sessionEpoch,
      executionEvent: {
        schemaVersion: 1,
        eventId,
        runScope: 'standalone',
        runId: 'run-1',
        sessionId: 'session-1',
        createdAtMs: 1_700_000_000_000,
        stateAfter: 'running',
        bodyJson: new TextEncoder().encode(
          JSON.stringify({ kind: 'tool_call', tool_name: 'shell' })
        )
      }
    }).finish()
  )
}

function taskCheckpointFrame(sequenceId: number): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      sequenceId,
      taskId: 'task-1',
      eventType: 'task.checkpoint',
      coreInstanceId: CORE_INSTANCE,
      sessionEpoch: SESSION_EPOCH,
      taskCheckpoint: {
        schemaVersion: 1,
        checkpointId: 'checkpoint-1',
        taskId: 'task-1',
        status: 'blocked',
        recoveryDisposition: 'blocked',
        recoveryWarning: 'explicit reconciliation',
        blockers: ['unknown outcome'],
        refs: [{ kind: 'policy_snapshot', id: 'policy-v1', sensitivity: 'public' }]
      }
    }).finish()
  )
}

function skillCatalogFrame(sequenceId: number): Uint8Array {
  return encodeFrame(
    EventEnvelope.encode({
      protocol: { major: 1, minor: 0 },
      sequenceId,
      eventType: 'skills.catalog',
      coreInstanceId: CORE_INSTANCE,
      sessionEpoch: SESSION_EPOCH,
      skillCatalog: {
        schemaVersion: 1,
        skills: [{
          schemaVersion: 1,
          skillId: 'reviewer',
          name: 'Reviewer',
          description: 'Bounded review instructions',
          version: '1.0.0',
          scope: 'project',
          sourceKind: 'project_native',
          sourceRef: '.agents/skills/reviewer/SKILL.md',
          contentHash: 'hash-reviewer',
          allowedTools: ['workspace.read'],
          requiredCapabilities: ['workspace.read'],
          disableModelInvocation: true,
          referenceCount: 1,
          validationStatus: 'valid',
          validationErrorCode: '',
          warnings: []
        }],
        diagnostics: []
      }
    }).finish()
  )
}

const TEST_SECRET = 'ab'.repeat(32)

function launchContext(pipeName: string, secret = ''): LaunchContext {
  return {
    pipeName,
    clientId: 'test-shell',
    sessionId: 'test-session',
    clientRole: 'shell',
    secret,
    livenessEvent: '',
    developerLaunch: secret.length === 0
  }
}

function createClient(pipeName: string, secret = ''): CorePipeClient {
  const created = new CorePipeClient({
    launch: launchContext(pipeName, secret),
    connectTimeoutMs: 2_000,
    handshakeTimeoutMs: 2_000,
    backoff: { baseMs: 20, maxMs: 60, jitterRatio: 0 }
  })
  client = created
  return created
}

function waitForState(
  target: CorePipeClient,
  predicate: (state: ShellState) => boolean
): Promise<ShellState> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('timed out waiting for state')), 5_000)
    const listener = (state: ShellState): void => {
      if (predicate(state)) {
        clearTimeout(timer)
        target.off('state', listener)
        resolve(state)
      }
    }
    target.on('state', listener)
  })
}

function waitForEvent(
  target: CorePipeClient,
  predicate: (event: CoreEvent) => boolean
): Promise<CoreEvent> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('timed out waiting for event')), 5_000)
    const listener = (event: CoreEvent): void => {
      if (predicate(event)) {
        clearTimeout(timer)
        target.off('core-event', listener)
        resolve(event)
      }
    }
    target.on('core-event', listener)
  })
}

describe.runIf(process.platform === 'win32')('core pipe client', () => {
  it('handshakes and reports the negotiated protocol', async () => {
    const pipeName = uniquePipeName()
    let requestedResync = false
    server = await startStubCore(pipeName, {
      onCommand: (command) => {
        if (command.handshake) return [readyFrame()]
        if (command.resyncRequest) requestedResync = true
        return []
      }
    })

    const target = createClient(pipeName)
    const connected = waitForState(target, (state) => state.connection === 'connected')
    target.start()
    const state = await connected

    expect(state.protocol).toEqual({ major: 1, minor: 0 })
    expect(state.coreVersion).toBe('0.1.0-test')
    expect(state.capabilities).toEqual(['replay', 'resync', 'skills', 'task_checkpoint'])
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(requestedResync).toBe(true)
  })

  it('streams events and tracks the last sequence', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) =>
        command.handshake
          ? [readyFrame(), eventFrame(1, 'task.started'), eventFrame(2, 'task.completed')]
          : []
    })

    const target = createClient(pipeName)
    const completed = waitForEvent(target, (event) => event.eventType === 'task.completed')
    target.start()
    const event = await completed

    expect(event.sequenceId).toBe(2)
    expect(target.state.lastSequence).toBe(2)
    expect(event.executionEvent).toBeNull()
  })

  it('projects a typed ledger event into executionEvent', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) =>
        command.handshake ? [readyFrame(), ledgerEventFrame(1, 'evt-single-1')] : []
    })

    const target = createClient(pipeName)
    const received = waitForEvent(target, (event) => event.executionEvent?.eventId === 'evt-single-1')
    target.start()
    const event = await received

    expect(event.eventType).toBe('ledger.tool_call')
    expect(event.executionEvent?.runScope).toBe('standalone')
    expect(event.executionEvent?.stateAfter).toBe('running')
    expect(event.executionEvent?.body).toEqual({ kind: 'tool_call', tool_name: 'shell' })
  })

  it('projects a typed TaskCheckpoint response without generic payload parsing', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) => (command.handshake ? [readyFrame(), taskCheckpointFrame(1)] : [])
    })

    const target = createClient(pipeName)
    const received = waitForEvent(target, (event) => event.taskCheckpoint?.checkpointId === 'checkpoint-1')
    target.start()
    const event = await received

    expect(event.payload).toBe('')
    expect(event.taskCheckpoint?.recoveryDisposition).toBe('blocked')
    expect(event.taskCheckpoint?.refs[0]).toMatchObject({ kind: 'policy_snapshot', id: 'policy-v1' })
    expect(event.taskCheckpointAction).toBeNull()
  })

  it('projects typed Agent Skills metadata and keeps the generic payload empty', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) => (command.handshake ? [readyFrame(), skillCatalogFrame(1)] : [])
    })

    const target = createClient(pipeName)
    const received = waitForEvent(target, (event) => event.skillCatalog?.skills[0]?.skillId === 'reviewer')
    target.start()
    const event = await received

    expect(event.payload).toBe('')
    expect(event.skillCatalog?.skills[0]).toMatchObject({
      skillId: 'reviewer',
      sourceKind: 'project_native',
      contentHash: 'hash-reviewer'
    })
  })

  it('suppresses a repeat delivery of the same typed ledger event_id', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) =>
        command.handshake
          ? [readyFrame(), ledgerEventFrame(1, 'evt-dup-1'), ledgerEventFrame(2, 'evt-dup-1')]
          : []
    })

    const target = createClient(pipeName)
    const received: CoreEvent[] = []
    target.on('core-event', (event) => {
      if (event.executionEvent?.eventId === 'evt-dup-1') {
        received.push(event)
      }
    })
    target.start()
    await waitForState(target, (state) => state.connection === 'connected')
    await new Promise((resolve) => setTimeout(resolve, 50))

    expect(received).toHaveLength(1)
  })

  it('keeps suppressing a repeat ledger event_id after a Core generation change', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) =>
        command.handshake
          ? [
              readyFrame(),
              ledgerEventFrame(1, 'evt-epoch-1'),
              // Same durable event_id re-delivered under a new Core instance/epoch.
              ledgerEventFrame(1, 'evt-epoch-1', 'core-instance-2', SESSION_EPOCH + 1)
            ]
          : []
    })

    const target = createClient(pipeName)
    const received: CoreEvent[] = []
    target.on('core-event', (event) => {
      if (event.executionEvent?.eventId === 'evt-epoch-1') {
        received.push(event)
      }
    })
    target.start()
    await waitForState(target, (state) => state.connection === 'resyncing')
    await new Promise((resolve) => setTimeout(resolve, 50))

    expect(received).toHaveLength(1)
  })

  it('refuses an incompatible major version instead of falling back', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) =>
        command.handshake
          ? [
              encodeFrame(
                EventEnvelope.encode({
                  protocol: { major: 2, minor: 0 },
                  eventType: 'core.ready',
                  ready: { protocol: { major: 2, minor: 0 }, coreVersion: '9.9.9' }
                }).finish()
              )
            ]
          : []
    })

    const target = createClient(pipeName)
    const mismatch = waitForState(target, (state) => state.connection === 'version-mismatch')
    target.start()

    expect((await mismatch).protocol).toBeNull()
  })

  it('treats an oversized frame as a protocol error and reconnects', async () => {
    const pipeName = uniquePipeName()
    let connections = 0
    server = await startStubCore(pipeName, {
      onConnection: () => {
        connections += 1
      },
      onCommand: (command, socket) => {
        if (!command.handshake) {
          return []
        }
        if (connections === 1) {
          // Announce a length above the bounded frame limit.
          const header = Buffer.alloc(4)
          header.writeUInt32LE(8 * 1024 * 1024, 0)
          socket.write(header)
          return []
        }
        return [readyFrame()]
      }
    })

    const target = createClient(pipeName)
    const recovered = waitForState(
      target,
      (state) => state.connection === 'connected' && state.reconnectAttempts === 0
    )
    target.start()
    await recovered

    expect(connections).toBeGreaterThanOrEqual(2)
  })

  it('requests a resync when a sequence is skipped', async () => {
    const pipeName = uniquePipeName()
    const resyncRequests: number[] = []
    server = await startStubCore(pipeName, {
      onCommand: (command) => {
        if (command.handshake) {
          return [readyFrame(), eventFrame(1, 'task.started'), eventFrame(5, 'task.progress')]
        }
        if (command.resyncRequest) {
          resyncRequests.push(Number(command.resyncRequest.afterSequence ?? 0))
        }
        return []
      }
    })

    const target = createClient(pipeName)
    const gap = waitForState(target, (state) => state.connection === 'state-gap')
    target.start()
    const state = await gap

    expect(state.reason).toBe('sequence-skipped')
    await new Promise((resolve) => setTimeout(resolve, 50))
    expect(resyncRequests).toContain(1)
  })

  it('does not queue a second auto-resync for the same afterSequence while one is in flight', async () => {
    const pipeName = uniquePipeName()
    const resyncRequests: number[] = []
    server = await startStubCore(pipeName, {
      onCommand: (command) => {
        if (command.handshake) {
          // event(1) establishes a lastSequence baseline first — a fresh
          // client (lastSequence 0) never treats its very first event as a
          // gap. events(10,11,12) then land in the same data chunk, all far
          // ahead of that baseline: each independently detects a gap, but
          // only the first should turn into a fresh resync request — the
          // other two share its still-unanswered afterSequence.
          return [
            readyFrame(),
            eventFrame(1, 'task.a'),
            eventFrame(10, 'task.b'),
            eventFrame(11, 'task.c'),
            eventFrame(12, 'task.d')
          ]
        }
        if (command.resyncRequest) {
          resyncRequests.push(Number(command.resyncRequest.afterSequence ?? 0))
        }
        return []
      }
    })

    const target = createClient(pipeName)
    const gap = waitForState(target, (state) => state.connection === 'state-gap')
    target.start()
    await gap

    await new Promise((resolve) => setTimeout(resolve, 50))
    // [0] is the unrelated resync issued right after connect (plan 0);
    // [1] is the one gap-triggered request all three skipped events share.
    expect(resyncRequests).toEqual([0, 1])
  })

  it('chains resync pages while Core reports more history, then settles once caught up', async () => {
    const pipeName = uniquePipeName()
    const resyncRequests: number[] = []
    server = await startStubCore(pipeName, {
      onCommand: (command) => {
        if (command.handshake) {
          return [readyFrame()]
        }
        if (command.resyncRequest) {
          const after = Number(command.resyncRequest.afterSequence ?? 0)
          resyncRequests.push(after)
          if (after === 0) {
            return [eventFrame(1, 'task.a'), eventFrame(2, 'task.b'), resyncEndFrame(2, true)]
          }
          return [eventFrame(3, 'task.c'), resyncEndFrame(3, false)]
        }
        return []
      }
    })

    const target = createClient(pipeName)
    const connected = waitForState(target, (state) => state.connection === 'connected' && state.lastSequence === 3)
    target.start()
    const state = await connected

    expect(resyncRequests).toEqual([0, 2])
    expect(state.lastSequence).toBe(3)
  })

  it('reconnects with bounded backoff after Core disconnects', async () => {
    const pipeName = uniquePipeName()
    let connections = 0
    server = await startStubCore(pipeName, {
      onConnection: () => {
        connections += 1
      },
      onCommand: (command, socket) => {
        if (!command.handshake) {
          return []
        }
        if (connections === 1) {
          socket.destroy()
          return []
        }
        return [readyFrame()]
      }
    })

    const target = createClient(pipeName)
    const reconnecting = waitForState(target, (state) => state.connection === 'reconnecting')
    target.start()
    await reconnecting
    await waitForState(target, (state) => state.connection === 'connected')

    expect(connections).toBeGreaterThanOrEqual(2)
  })

  it('answers the nonce with an HMAC proof bound to role and client id', async () => {
    const pipeName = uniquePipeName()
    let proof = ''
    let role = ''
    let nonce = ''
    server = await startStubCore(pipeName, {
      onCommand: (command) => {
        if (!command.handshake) {
          return []
        }
        proof = command.handshake.proof ?? ''
        role = command.handshake.clientRole ?? ''
        nonce = command.handshake.nonce ?? ''
        return [readyFrame()]
      }
    })

    const target = createClient(pipeName, TEST_SECRET)
    const connected = waitForState(target, (state) => state.connection === 'connected')
    target.start()
    await connected

    expect(nonce).toBe(TEST_NONCE)
    expect(role).toBe('shell')
    expect(proof).toBe(
      createHmac('sha256', Buffer.from(TEST_SECRET, 'utf8'))
        .update(`shell
test-shell
${TEST_NONCE}`, 'utf8')
        .digest('hex')
    )
  })

  it('sends nothing before Core issues a nonce', async () => {
    const pipeName = uniquePipeName()
    let commands = 0
    server = await startStubCore(pipeName, {
      issueChallenge: false,
      onCommand: () => {
        commands += 1
        return []
      }
    })

    const target = createClient(pipeName, TEST_SECRET)
    target.start()
    await new Promise((resolve) => setTimeout(resolve, 200))

    expect(commands).toBe(0)
    expect(target.state.connection).not.toBe('connected')
  })

  it('stops retrying after Core keeps rejecting the handshake', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, {
      onCommand: (command) => (command.handshake ? [rejectedFrame()] : [])
    })

    const target = createClient(pipeName, TEST_SECRET)
    const fatal = waitForState(target, (state) => state.connection === 'fatal')
    target.start()

    expect((await fatal).reason).toBe('auth-rejected')
  })

  it('rejects commands with a controlled failure once the queue is full', async () => {
    const pipeName = uniquePipeName()
    server = await startStubCore(pipeName, { onCommand: () => [] })

    const target = createClient(pipeName)
    // Never started: the socket is absent, so everything stays queued and the
    // bounded queue must reject rather than grow without limit.
    const results = new Set<string>()
    for (let index = 0; index < 400; index += 1) {
      results.add(target.send({ stopTask: { taskId: `task-${index}` } }))
    }
    expect(results.has('queued')).toBe(true)
    expect(results.has('queue-full')).toBe(true)
  })
})
