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
}

function uniquePipeName(): string {
  pipeIndex += 1
  return `\\\\.\\pipe\\evohime-test-${process.pid}-${pipeIndex}`
}

async function startStubCore(pipeName: string, options: StubOptions): Promise<Server> {
  const stub = createServer((socket) => {
    const reader = new FrameReader()
    options.onConnection?.(socket)
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

function launchContext(pipeName: string): LaunchContext {
  return {
    pipeName,
    clientId: 'test-shell',
    sessionId: 'test-session',
    challenge: '',
    livenessEvent: '',
    developerLaunch: true
  }
}

function createClient(pipeName: string): CorePipeClient {
  const created = new CorePipeClient({
    launch: launchContext(pipeName),
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
    server = await startStubCore(pipeName, {
      onCommand: (command) => (command.handshake ? [readyFrame()] : [])
    })

    const target = createClient(pipeName)
    const connected = waitForState(target, (state) => state.connection === 'connected')
    target.start()
    const state = await connected

    expect(state.protocol).toEqual({ major: 1, minor: 0 })
    expect(state.coreVersion).toBe('0.1.0-test')
    expect(state.capabilities).toEqual(['replay', 'resync'])
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
