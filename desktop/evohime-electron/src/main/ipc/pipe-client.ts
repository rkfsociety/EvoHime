import { EventEmitter } from 'node:events'
import { connect, type Socket } from 'node:net'
import { randomUUID } from 'node:crypto'

import type {
  ConnectionState,
  CoreEvent,
  ProtocolVersion,
  ShellState,
  TypedExecutionEvent
} from '@shared/api'

import type { ShellLog } from '../diagnostics/logger'
import { backoffDelayMs, DEFAULT_BACKOFF, type BackoffOptions } from './backoff'
import { CommandQueue, type EnqueueResult } from './command-queue'
import { encodeFrame, FrameError, MAX_FRAME_BYTES } from './frame-codec'
import { FrameReader } from './frame-reader'
import { evohime } from './generated/protocol.js'
import { handshakeProof, type LaunchContext } from './launch-context'
import { LedgerEventDedup } from './ledger-event-dedup'
import { LOCAL_PROTOCOL, negotiateProtocol, NegotiationError } from './protocol-version'

const { CommandEnvelope, EventEnvelope } = evohime.desktop.v1

type ICommandEnvelope = evohime.desktop.v1.ICommandEnvelope

/**
 * The single place in the Electron main process that speaks `desktop-ipc-v1`.
 *
 * It owns connect/handshake/reconnect, sequence tracking, resync, bounded
 * frames and the bounded command queue. It carries no Core business logic and
 * makes no security decision: Core re-validates every command it forwards
 * (plan 0, stage 1).
 */

export const CLIENT_CAPABILITIES = ['replay', 'resync'] as const

export const DEFAULT_CONNECT_TIMEOUT_MS = 5_000
export const DEFAULT_HANDSHAKE_TIMEOUT_MS = 5_000

export const MAX_AUTH_REJECTIONS = 3

export interface PipeClientOptions {
  readonly launch: LaunchContext
  /**
   * Re-reads the launch context before each connection attempt so a rotated
   * supervisor session is picked up without restarting the shell.
   */
  readonly refreshLaunch?: () => LaunchContext
  readonly connectTimeoutMs?: number
  readonly handshakeTimeoutMs?: number
  readonly backoff?: BackoffOptions
  /** Injectable socket factory; tests substitute an in-memory pipe. */
  readonly createSocket?: (pipeName: string) => Socket
  readonly log?: ShellLog
}

export interface PipeClientEvents {
  state: [ShellState]
  'core-event': [CoreEvent]
}

export class CorePipeClient extends EventEmitter<PipeClientEvents> {
  private socket: Socket | null = null
  private readonly reader = new FrameReader()
  private readonly queue = new CommandQueue()
  private readonly options: Required<
    Pick<PipeClientOptions, 'connectTimeoutMs' | 'handshakeTimeoutMs' | 'backoff'>
  >
  private launch: LaunchContext
  private readonly refreshLaunch: (() => LaunchContext) | null
  private readonly createSocket: (pipeName: string) => Socket
  private readonly log: NonNullable<PipeClientOptions['log']>

  private connection: ConnectionState = 'starting'
  private protocol: ProtocolVersion | null = null
  private capabilities: string[] = []
  private coreVersion: string | null = null
  private lastSequence = 0
  private reason: string | null = null
  private reconnectAttempts = 0
  private coreInstanceId = ''
  private sessionEpoch = 0
  private stopped = true
  private writable = true
  private reconnectTimer: NodeJS.Timeout | null = null
  private handshakeTimer: NodeJS.Timeout | null = null
  private authRejections = 0
  /**
   * Durable across a session-epoch change on purpose: `event_id` is stable
   * between Core generations, unlike `sequence_id` (plan 08-3).
   */
  private readonly ledgerEventDedup = new LedgerEventDedup()

  constructor(options: PipeClientOptions) {
    super()
    this.launch = options.launch
    this.refreshLaunch = options.refreshLaunch ?? null
    this.options = {
      connectTimeoutMs: options.connectTimeoutMs ?? DEFAULT_CONNECT_TIMEOUT_MS,
      handshakeTimeoutMs: options.handshakeTimeoutMs ?? DEFAULT_HANDSHAKE_TIMEOUT_MS,
      backoff: options.backoff ?? DEFAULT_BACKOFF
    }
    this.createSocket =
      options.createSocket ?? ((pipeName) => connect({ path: pipeName, allowHalfOpen: false }))
    this.log = options.log ?? (() => {})
  }

  get state(): ShellState {
    return {
      connection: this.connection,
      protocol: this.protocol,
      capabilities: [...this.capabilities],
      coreVersion: this.coreVersion,
      lastSequence: this.lastSequence,
      reason: this.reason,
      reconnectAttempts: this.reconnectAttempts
    }
  }

  start(): void {
    if (!this.stopped) {
      return
    }
    this.stopped = false
    this.openConnection()
  }

  stop(): void {
    this.stopped = true
    this.clearTimers()
    this.destroySocket()
    this.setState('starting', null)
  }

  /**
   * Enqueues one command for Core. Returns `queue-full` instead of dropping it
   * silently; the caller surfaces that to the renderer as a typed failure.
   */
  send(command: ICommandEnvelope): EnqueueResult {
    const envelope: ICommandEnvelope = {
      protocol: LOCAL_PROTOCOL,
      requestId: command.requestId ?? randomUUID(),
      clientId: this.launch.clientId,
      coreInstanceId: this.coreInstanceId,
      sessionEpoch: this.sessionEpoch,
      ...command
    }
    let frame: Uint8Array
    try {
      frame = encodeFrame(CommandEnvelope.encode(envelope).finish())
    } catch (error) {
      this.log('warn', 'ipc.command_encode_failed', { error })
      return 'queue-full'
    }
    const result = this.queue.enqueue({ requestId: envelope.requestId ?? '', frame })
    if (result === 'queued') {
      this.flushQueue()
    } else {
      this.log('warn', 'ipc.command_queue_full', { size: this.queue.size, bytes: this.queue.bytes })
    }
    return result
  }

  /** Asks Core for a bounded resync, preferring an atomic snapshot. */
  requestResync(includeFullSnapshot = true): EnqueueResult {
    return this.send({
      resyncRequest: {
        afterSequence: this.lastSequence,
        maxEvents: 0,
        includeFullSnapshot
      }
    })
  }

  private openConnection(): void {
    if (this.stopped || this.socket) {
      return
    }
    if (this.refreshLaunch) {
      try {
        this.launch = this.refreshLaunch()
      } catch (error) {
        this.log('warn', 'ipc.launch_context_unreadable', { error })
      }
    }
    this.setState(this.reconnectAttempts === 0 ? 'connecting' : 'reconnecting', null)
    this.reader.reset()

    let socket: Socket
    try {
      socket = this.createSocket(this.launch.pipeName)
    } catch (error) {
      this.scheduleReconnect('connect-failed', error)
      return
    }
    this.socket = socket
    this.writable = true

    const connectTimer = setTimeout(() => {
      this.log('warn', 'ipc.connect_timeout', {})
      socket.destroy()
    }, this.options.connectTimeoutMs)

    socket.once('connect', () => {
      clearTimeout(connectTimer)
      this.onConnected()
    })
    socket.on('data', (chunk: Buffer) => this.onData(chunk))
    socket.on('drain', () => {
      this.writable = true
      this.flushQueue()
    })
    socket.once('error', (error) => {
      clearTimeout(connectTimer)
      this.scheduleReconnect('socket-error', error)
    })
    socket.once('close', () => {
      clearTimeout(connectTimer)
      this.scheduleReconnect('socket-closed', null)
    })
  }

  /**
   * Core speaks first: it issues a single-use nonce, and only then does the
   * shell send a handshake carrying the proof. Nothing is sent before the
   * challenge arrives.
   */
  private onConnected(): void {
    this.handshakeTimer = setTimeout(() => {
      this.log('warn', 'ipc.handshake_timeout', {})
      this.destroySocket()
      this.scheduleReconnect('handshake-timeout', null)
    }, this.options.handshakeTimeoutMs)
  }

  private answerChallenge(nonce: string, expiresAtMs: number): void {
    if (expiresAtMs > 0 && Date.now() > expiresAtMs) {
      this.log('warn', 'ipc.challenge_expired', {})
      this.destroySocket()
      this.scheduleReconnect('challenge-expired', null)
      return
    }
    this.send({
      handshake: {
        protocol: LOCAL_PROTOCOL,
        clientId: this.launch.clientId,
        sessionId: this.launch.sessionId,
        sessionEpoch: this.sessionEpoch,
        lastEventSequence: this.lastSequence,
        capabilities: [...CLIENT_CAPABILITIES],
        clientRole: this.launch.clientRole,
        nonce,
        proof: handshakeProof(this.launch, nonce)
      }
    })
  }

  /**
   * Core refused the handshake. Retrying is only useful if the launch context
   * changed, so a few attempts are allowed before the shell stops and shows a
   * recovery state instead of looping.
   */
  private onAuthRejected(reason: string): void {
    this.authRejections += 1
    this.log('error', 'ipc.auth_rejected', { attempt: this.authRejections })
    this.destroySocket()
    if (this.authRejections >= MAX_AUTH_REJECTIONS) {
      this.setState('fatal', reason)
      this.stop()
      return
    }
    this.setState('degraded', reason)
    this.scheduleReconnect('auth-rejected', null)
  }

  private onData(chunk: Buffer): void {
    let frames: Uint8Array[]
    try {
      frames = this.reader.push(chunk)
    } catch (error) {
      // An oversized or malformed frame is a protocol error, never a partial
      // state update: drop the connection instead of applying anything.
      this.log('error', 'ipc.frame_rejected', {
        error,
        maxFrameBytes: MAX_FRAME_BYTES
      })
      this.setState('degraded', error instanceof FrameError ? error.kind : 'frame-error')
      this.destroySocket()
      this.scheduleReconnect('frame-error', error)
      return
    }
    for (const frame of frames) {
      this.onFrame(frame)
    }
  }

  private onFrame(frame: Uint8Array): void {
    let event: evohime.desktop.v1.EventEnvelope
    try {
      event = EventEnvelope.decode(frame)
    } catch (error) {
      this.log('error', 'ipc.event_decode_failed', { error })
      this.setState('degraded', 'protocol-error')
      return
    }

    if (event.authChallenge) {
      this.answerChallenge(
        event.authChallenge.nonce ?? '',
        Number(event.authChallenge.expiresAtMs ?? 0)
      )
      return
    }
    if (event.eventType === 'ipc.rejected') {
      this.onAuthRejected('auth-rejected')
      return
    }
    if (event.ready) {
      this.onReady(event)
      return
    }
    if (this.isEpochChanged(event)) {
      this.onEpochChanged(event)
    }
    if (event.replayGap) {
      this.log('warn', 'ipc.replay_gap', {
        requested: event.replayGap.requestedAfterSequence,
        earliest: event.replayGap.earliestAvailableSequence,
        latest: event.replayGap.latestAvailableSequence,
        reason: event.replayGap.reason || 'unspecified'
      })
      this.setState('state-gap', event.replayGap.reason || 'replay-gap')
      this.requestResync(true)
      return
    }
    if (event.fullSnapshot) {
      this.lastSequence = Number(event.fullSnapshot.sequenceId ?? this.lastSequence)
      this.setState('connected', null)
      this.emitCoreEvent(event)
      return
    }

    if (event.eventType === 'resync.end' || event.eventType === 'replay.end') {
      this.setState('connected', null)
    }

    const sequence = Number(event.sequenceId ?? 0)
    if (sequence > 0) {
      if (sequence > this.lastSequence + 1 && this.lastSequence > 0) {
        // A skipped sequence is never treated as a successful recovery.
        this.log('warn', 'ipc.sequence_skipped', { expected: this.lastSequence + 1, sequence })
        this.setState('state-gap', 'sequence-skipped')
        this.requestResync(true)
        return
      }
      this.lastSequence = Math.max(this.lastSequence, sequence)
    }
    if (!this.shouldEmit(event)) {
      return
    }
    this.emitCoreEvent(event)
  }

  /**
   * Suppresses a repeat delivery of the same typed ledger event, identified
   * by its durable `event_id` (plan 08-3). Generic (non-typed) rows have no
   * `event_id` and are always emitted — the existing sequence/gap checks
   * above are what guards them.
   */
  private shouldEmit(event: evohime.desktop.v1.EventEnvelope): boolean {
    const eventId = event.executionEvent?.eventId
    if (!eventId) {
      return true
    }
    return this.ledgerEventDedup.observe(eventId)
  }

  private onReady(event: evohime.desktop.v1.EventEnvelope): void {
    if (this.handshakeTimer) {
      clearTimeout(this.handshakeTimer)
      this.handshakeTimer = null
    }
    const peer = event.ready?.protocol ?? event.protocol
    try {
      const negotiated = negotiateProtocol(
        LOCAL_PROTOCOL,
        { major: Number(peer?.major ?? 0), minor: Number(peer?.minor ?? 0) },
        CLIENT_CAPABILITIES,
        [...CLIENT_CAPABILITIES]
      )
      this.protocol = negotiated.version
      this.capabilities = negotiated.capabilities
    } catch (error) {
      // A different major is a hard stop: the shell shows a recovery state and
      // never falls back to an unknown scheme.
      this.log('error', 'ipc.version_mismatch', { error })
      this.protocol = null
      this.setState(
        'version-mismatch',
        error instanceof NegotiationError ? error.kind : 'negotiation-failed'
      )
      this.stop()
      return
    }

    this.coreVersion = event.ready?.coreVersion ?? null
    this.coreInstanceId = event.coreInstanceId ?? ''
    this.sessionEpoch = Number(event.sessionEpoch ?? 0)
    this.reconnectAttempts = 0
    this.authRejections = 0
    this.setState('connected', null)

    // A fresh Electron process has no local sequence cursor yet. Replay the
    // bounded Core journal as well, otherwise the renderer starts with an
    // empty trace after every restart and can only observe future events.
    this.setState('replaying', null)
    this.requestResync(false)
    this.flushQueue()
  }

  private isEpochChanged(event: evohime.desktop.v1.EventEnvelope): boolean {
    const instanceId = event.coreInstanceId ?? ''
    const epoch = Number(event.sessionEpoch ?? 0)
    if (instanceId.length === 0 && epoch === 0) {
      return false
    }
    return (
      (this.coreInstanceId.length > 0 && instanceId !== this.coreInstanceId) ||
      (this.sessionEpoch > 0 && epoch !== this.sessionEpoch)
    )
  }

  private onEpochChanged(event: evohime.desktop.v1.EventEnvelope): void {
    this.log('warn', 'ipc.session_epoch_changed', {
      previousEpoch: this.sessionEpoch,
      epoch: Number(event.sessionEpoch ?? 0)
    })
    this.coreInstanceId = event.coreInstanceId ?? ''
    this.sessionEpoch = Number(event.sessionEpoch ?? 0)
    this.lastSequence = 0
    for (const dropped of this.queue.drain()) {
      this.log('warn', 'ipc.command_dropped_on_epoch_change', { requestId: dropped.requestId })
    }
    this.setState('resyncing', 'session-epoch-changed')
  }

  private emitCoreEvent(event: evohime.desktop.v1.EventEnvelope): void {
    this.emit('core-event', {
      sequenceId: Number(event.sequenceId ?? 0),
      taskId: event.taskId ?? '',
      eventType: event.eventType ?? '',
      payload: decodePayload(event.payload),
      executionEvent: decodeExecutionEvent(event.executionEvent)
    })
  }

  private flushQueue(): void {
    const socket = this.socket
    if (!socket || socket.destroyed || !socket.writable) {
      return
    }
    while (this.writable) {
      const command = this.queue.dequeue()
      if (!command) {
        return
      }
      this.writable = socket.write(command.frame)
    }
  }

  private scheduleReconnect(reason: string, error: unknown): void {
    this.destroySocket()
    if (this.stopped || this.connection === 'version-mismatch' || this.connection === 'fatal') {
      return
    }
    if (this.reconnectTimer) {
      // `error` and `close` both fire for one broken socket; a single attempt
      // must not be counted or scheduled twice.
      return
    }
    this.reconnectAttempts += 1
    this.setState('reconnecting', reason)
    this.log('warn', 'ipc.reconnect_scheduled', { reason, error, attempt: this.reconnectAttempts })

    const delay = backoffDelayMs(this.reconnectAttempts - 1, this.options.backoff)
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      this.openConnection()
    }, delay)
    this.reconnectTimer.unref?.()
  }

  private destroySocket(): void {
    const socket = this.socket
    this.socket = null
    if (this.handshakeTimer) {
      clearTimeout(this.handshakeTimer)
      this.handshakeTimer = null
    }
    if (socket) {
      socket.removeAllListeners()
      socket.destroy()
    }
  }

  private clearTimers(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
  }

  private setState(connection: ConnectionState, reason: string | null): void {
    if (this.connection === connection && this.reason === reason) {
      return
    }
    this.connection = connection
    this.reason = reason
    this.emit('state', this.state)
  }
}

function decodePayload(payload: Uint8Array | null | undefined): string {
  if (!payload || payload.byteLength === 0) {
    return ''
  }
  return Buffer.from(payload).toString('utf8')
}

/**
 * Decodes the additive `ExecutionEvent` oneof (plan 08-3) into the
 * renderer-facing shape. `body` is parsed once here so downstream code never
 * repeats a possibly-malformed JSON.parse; a decode failure degrades to
 * `null` rather than dropping the whole frame.
 */
function decodeExecutionEvent(
  projected: evohime.desktop.v1.IExecutionEvent | null | undefined
): TypedExecutionEvent | null {
  if (!projected || !projected.eventId) {
    return null
  }
  let body: unknown = null
  if (projected.bodyJson && projected.bodyJson.byteLength > 0) {
    try {
      body = JSON.parse(Buffer.from(projected.bodyJson).toString('utf8'))
    } catch {
      body = null
    }
  }
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    eventId: projected.eventId,
    runScope: projected.runScope ?? '',
    runId: projected.runId ?? '',
    sessionId: projected.sessionId ?? '',
    createdAtMs: Number(projected.createdAtMs ?? 0),
    stateAfter: projected.stateAfter ?? '',
    actionId: projected.actionId ?? '',
    toolCallId: projected.toolCallId ?? '',
    observationId: projected.observationId ?? '',
    receiptId: projected.receiptId ?? '',
    failureId: projected.failureId ?? '',
    workflowRunId: projected.workflowRunId ?? '',
    nodeId: projected.nodeId ?? '',
    attemptId: projected.attemptId ?? '',
    effectId: projected.effectId ?? '',
    modelRequestId: projected.modelRequestId ?? '',
    body,
    secretsPresent: Boolean(projected.secretsPresent),
    redactionDigest: projected.redactionDigest ?? ''
  }
}
