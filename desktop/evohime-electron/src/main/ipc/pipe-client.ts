import { EventEmitter } from 'node:events'
import { connect, type Socket } from 'node:net'
import { randomUUID } from 'node:crypto'

import type {
  ConnectionState,
  ConversationEventLogPage,
  ConversationEventProjection,
  ConversationWorkbenchProjection,
  CapabilityWorkbenchProjection,
  TeamCoordinatorProjection,
  ProjectInstructionStackProjection,
  WorkspaceSetsProjection,
  KnowledgeSourceRegistryProjection,
  AgentGitChangeSetsProjection,
  ArchitectEditorPipelineProjection,
  EventVisualizerRegistryProjection,
  ReasoningOperatorLibraryProjection,
  ContinuationActionResult,
  ContinuationProjection,
  AnalysisKernelProjection,
  AnalysisKernelResult,
  RefinementActionResult,
  RefinementProjection,
  CoreAvailabilityCode,
  CoreEvent,
  GoalActionResult,
  GoalCriterion,
  GoalListProjection,
  GoalProjection,
  SkillCatalog,
  SkillContentResult,
  SkillDiagnostic,
  SkillMetadata,
  SkillReferenceResult,
  ProtocolVersion,
  ShellState,
  TaskCheckpointActionResult,
  TaskCheckpointProjection,
  TaskCheckpointRef,
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
import {
  LOCAL_PROTOCOL,
  negotiateLimits,
  negotiateProtocol,
  NegotiationError,
  type EffectiveLimits
} from './protocol-version'

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

export const CLIENT_CAPABILITIES = ['conversation_event_log', 'conversation_workbench', 'replay', 'resync', 'task_checkpoint', 'skills', 'goals', 'workflow_builder'] as const

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
  private availability: CoreAvailabilityCode | null = null
  private limits: EffectiveLimits | null = null
  private reconnectAttempts = 0
  private coreInstanceId = ''
  private sessionEpoch = 0
  private stopped = true
  private writable = true
  private reconnectTimer: NodeJS.Timeout | null = null
  private handshakeTimer: NodeJS.Timeout | null = null
  private authRejections = 0
  /**
   * `afterSequence` of the auto-resync currently awaiting a response, or
   * `null` when none is in flight. A large backlog is paged 512 events at a
   * time (`DEFAULT_RESYNC_MAX_EVENTS` in Core); without this guard, a burst
   * of live events arriving mid-page each independently detected the same
   * gap and queued their own redundant resync request for the exact same
   * `afterSequence`, never converging on a busy session. Tracking the value
   * (not just a boolean) still lets a resync fire once `lastSequence` has
   * genuinely moved on since the in-flight request was sent.
   */
  private resyncPendingAfter: number | null = null
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
      availability: this.availability,
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
    if (this.limits && frame.byteLength - 4 > this.limits.maxFrameBytes) {
      this.log('warn', 'ipc.command_exceeds_peer_limit', {
        frameBytes: frame.byteLength - 4,
        maxFrameBytes: this.limits.maxFrameBytes
      })
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

  /**
   * Same as `requestResync`, but skips the call while one is already
   * in flight instead of queuing a redundant duplicate. Used by the
   * automatic gap-recovery paths (as opposed to the user-triggered
   * `shell.requestResync` command, which always sends immediately).
   */
  private autoResync(includeFullSnapshot: boolean): void {
    if (this.resyncPendingAfter === this.lastSequence) return
    this.resyncPendingAfter = this.lastSequence
    this.requestResync(includeFullSnapshot)
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
      this.autoResync(true)
      return
    }
    if (event.fullSnapshot) {
      this.resyncPendingAfter = null
      this.lastSequence = Number(event.fullSnapshot.sequenceId ?? this.lastSequence)
      this.setState('connected', null)
      this.emitCoreEvent(event)
      return
    }

    // Core pages a large backlog `DEFAULT_RESYNC_MAX_EVENTS` at a time and
    // says so on `resync.end`'s payload (`more_available`). Declaring
    // `connected` after just one page — while more history still sits
    // beyond it — is what let a busy session's live traffic race ahead of
    // catch-up forever; chaining the next page immediately here instead of
    // waiting for that race to surface as a sequence gap is what actually
    // converges.
    const resyncEnd =
      event.eventType === 'resync.end' || event.eventType === 'replay.end'
        ? parseResyncEnd(event)
        : null

    const sequence = Number(event.sequenceId ?? 0)
    if (sequence > 0) {
      if (sequence > this.lastSequence + 1 && this.lastSequence > 0) {
        // A skipped sequence is never treated as a successful recovery.
        this.log('warn', 'ipc.sequence_skipped', { expected: this.lastSequence + 1, sequence })
        this.setState('state-gap', 'sequence-skipped')
        this.autoResync(true)
        return
      }
      this.lastSequence = Math.max(this.lastSequence, sequence)
    }
    if (resyncEnd) {
      this.resyncPendingAfter = null
      if (resyncEnd.moreAvailable) {
        this.setState('resyncing', 'catching-up')
        this.autoResync(true)
      } else {
        this.setState('connected', null)
      }
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
    // Conversation pages may overlap by design. Suppressing an entire page by
    // its first event id would also drop unseen later events; the renderer
    // reducer de-duplicates each conversation event individually.
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
      const coreInfo = event.ready?.coreInfo
      this.limits = coreInfo
        ? negotiateLimits({
            maxFrameBytes: Number(coreInfo.maxFrameBytes ?? 0),
            maxReplayEvents: Number(coreInfo.maxReplayEvents ?? 0),
            maxSnapshotBytes: Number(coreInfo.maxSnapshotBytes ?? 0)
          })
        : {
            maxFrameBytes: 4 * 1024 * 1024,
            maxReplayEvents: 512,
            maxSnapshotBytes: 4 * 1024 * 1024 - 1024
          }
      const negotiated = negotiateProtocol(
        LOCAL_PROTOCOL,
        { major: Number(peer?.major ?? 0), minor: Number(peer?.minor ?? 0) },
        CLIENT_CAPABILITIES,
        coreInfo?.capabilities?.length ? [...coreInfo.capabilities] : [...CLIENT_CAPABILITIES]
      )
      this.protocol = negotiated.version
      this.capabilities = negotiated.capabilities
    } catch (error) {
      // A different major is a hard stop: the shell shows a recovery state and
      // never falls back to an unknown scheme.
      this.log('error', 'ipc.version_mismatch', { error })
      this.protocol = null
      this.limits = null
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
    this.autoResync(false)
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
      coreInstanceId: event.coreInstanceId ?? '',
      sessionEpoch: Number(event.sessionEpoch ?? 0),
      taskId: event.taskId ?? '',
      eventType: event.eventType ?? '',
      payload: decodePayload(event.payload),
      executionEvent: decodeExecutionEvent(event.executionEvent),
      taskCheckpoint: decodeTaskCheckpoint(event.taskCheckpoint),
      taskCheckpointAction: decodeTaskCheckpointAction(event.taskCheckpointActionResult),
      skillCatalog: decodeSkillCatalog(event.skillCatalog),
      skillContent: decodeSkillContent(event.skillContent),
      skillReference: decodeSkillReference(event.skillReference),
      goal: decodeGoal(event.goal),
      goalList: decodeGoalList(event.goalList),
      goalAction: decodeGoalAction(event.goalAction),
      continuation: decodeContinuation(event.continuation),
      continuationAction: decodeContinuationAction(event.continuationAction),
      analysisKernel: decodeAnalysisKernel(event.analysisKernel),
      analysisKernelResult: decodeAnalysisKernelResult(event.analysisKernelResult)
      , refinement: decodeRefinement(event.refinement)
      , refinementList: decodeRefinementList(event.refinementList)
      , refinementAction: decodeRefinementAction(event.refinementAction)
      , conversationEventLog: decodeConversationEventLog(event.conversationEventLog)
      , conversationWorkbench: decodeConversationWorkbench(event.conversationWorkbench)
      , capabilityWorkbench: decodeCapabilityWorkbench(event.capabilityWorkbench)
      , teamCoordinator: decodeTeamCoordinator(event.teamCoordinator)
      , projectInstructionStack: decodeProjectInstructionStack(event.projectInstructionStack)
      , workspaceSets: decodeWorkspaceSets(event.workspaceSets)
      , knowledgeSourceRegistry: decodeKnowledgeSourceRegistry(event.knowledgeSourceRegistry)
      , agentGitChangeSets: decodeAgentGitChangeSets(event.agentGitChangeSets)
      , architectEditorPipeline: decodeArchitectEditorPipeline(event.architectEditorPipeline)
      , eventVisualizerRegistry: decodeEventVisualizerRegistry(event.eventVisualizerRegistry)
      , reasoningOperatorLibrary: decodeReasoningOperatorLibrary(event.reasoningOperatorLibrary)
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
    // A pending flag from the dead connection must never block catch-up on
    // the next one — there is nothing left to answer it.
    this.resyncPendingAfter = null
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
    this.availability = availabilityFor(connection, reason)
    this.emit('state', this.state)
  }
}

function decodeContinuation(projected: evohime.desktop.v1.IContinuationProjection | null | undefined): ContinuationProjection | null {
  if (!projected || !projected.runId) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0), runId: projected.runId,
    ownerScope: projected.ownerScope ?? '', policyId: projected.policyId ?? '',
    policyRevision: Number(projected.policyRevision ?? 0), policyHash: projected.policyHash ?? '',
    state: projected.state ?? '', continuationIndex: Number(projected.continuationIndex ?? 0),
    maxContinuations: Number(projected.maxContinuations ?? 0), modelTurns: Number(projected.modelTurns ?? 0),
    maxModelTurns: Number(projected.maxModelTurns ?? 0), tokenUsed: Number(projected.tokenUsed ?? 0),
    costUsedMicros: Number(projected.costUsedMicros ?? 0), stopReason: projected.stopReason ?? '',
    errorCode: projected.errorCode ?? '',
    gates: (projected.gates ?? []).slice(0, 32).map((gate) => ({
      gateId: gate.gateId ?? '', kind: gate.kind ?? '', capabilityRef: gate.capabilityRef ?? '',
      status: gate.status ?? '', evidenceRef: gate.evidenceRef ?? '', errorCode: gate.errorCode ?? ''
    }))
  }
}

function decodeContinuationAction(projected: evohime.desktop.v1.IContinuationActionResult | null | undefined): ContinuationActionResult | null {
  if (!projected || !projected.runId) return null
  return { schemaVersion: Number(projected.schemaVersion ?? 0), runId: projected.runId, action: projected.action ?? '', applied: Boolean(projected.applied), deduplicated: Boolean(projected.deduplicated), errorCode: projected.errorCode ?? '' }
}

function availabilityFor(
  connection: ConnectionState,
  reason: string | null
): CoreAvailabilityCode | null {
  if (reason === 'session-epoch-changed' || reason === 'stale-session') return 'stale_session'
  if (connection === 'version-mismatch') return 'unsupported'
  if (connection === 'connecting' || connection === 'reconnecting' || connection === 'degraded') {
    return 'unavailable'
  }
  if (connection === 'fatal' || reason === 'protocol-error') return 'unknown'
  return null
}

function decodePayload(payload: Uint8Array | null | undefined): string {
  if (!payload || payload.byteLength === 0) {
    return ''
  }
  return Buffer.from(payload).toString('utf8')
}

function decodeConversationEventLog(
  projected: evohime.desktop.v1.IConversationEventLogEvent | null | undefined
): ConversationEventLogPage | null {
  if (!projected) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    operation: projected.operation ?? '',
    conversationId: projected.conversationId ?? '',
    events: (projected.events ?? []).map(decodeConversationEvent),
    oldestSequence: Number(projected.oldestSequence ?? 0),
    newestSequence: Number(projected.newestSequence ?? 0),
    hasOlder: Boolean(projected.hasOlder),
    hasNewer: Boolean(projected.hasNewer),
    earliestAvailableSequence: Number(projected.earliestAvailableSequence ?? 0),
    errorCode: projected.errorCode ?? ''
  }
}

function decodeConversationWorkbench(
  projected: evohime.desktop.v1.IConversationWorkbenchEvent | null | undefined
): ConversationWorkbenchProjection | null {
  if (!projected || projected.status !== 'ok' || !projected.projectionJson?.length) return null
  try {
    const value = JSON.parse(Buffer.from(projected.projectionJson).toString('utf8')) as Record<string, unknown>
    const tabs = Array.isArray(value['tabs']) ? value['tabs'] : []
    return {
      schemaVersion: Number(value['schema_version'] ?? 0),
      conversationId: typeof value['conversation_id'] === 'string' ? value['conversation_id'] : '',
      workspaceId: typeof value['workspace_id'] === 'string' ? value['workspace_id'] : '',
      runId: typeof value['run_id'] === 'string' ? value['run_id'] : '',
      backendSnapshotHash: typeof value['backend_snapshot_hash'] === 'string' ? value['backend_snapshot_hash'] : '',
      capabilitySnapshotHash: typeof value['capability_snapshot_hash'] === 'string' ? value['capability_snapshot_hash'] : '',
      eventCursor: Number(value['event_cursor'] ?? 0),
      eventCount: Number(value['event_count'] ?? 0),
      taskCount: Number(value['task_count'] ?? 0),
      usageInputTokens: Number(value['usage_input_tokens'] ?? 0),
      usageOutputTokens: Number(value['usage_output_tokens'] ?? 0),
      tabs: tabs.flatMap((tab): ConversationWorkbenchProjection['tabs'][number][] => {
        if (typeof tab !== 'object' || tab === null) return []
        const item = tab as Record<string, unknown>
        return [{ id: String(item['id'] ?? ''), label: String(item['label'] ?? ''), availability: String(item['availability'] ?? 'unavailable'), reason: String(item['reason'] ?? ''), badgeSource: String(item['badge_source'] ?? ''), persistence: String(item['persistence'] ?? 'presentation_only') }]
      }),
      redaction: typeof value['redaction'] === 'string' ? value['redaction'] : 'renderer_metadata_only'
    }
  } catch {
    return null
  }
}

function decodeCapabilityWorkbench(
  projected: evohime.desktop.v1.ICapabilityWorkbenchEvent | null | undefined
): CapabilityWorkbenchProjection | null {
  if (!projected) return null
  const raw = decodePayload(projected.projectionJson)
  let projection: unknown = null
  try { projection = JSON.parse(raw) } catch { projection = null }
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    instanceId: projected.instanceId ?? '',
    operation: projected.operation ?? '',
    revision: Number(projected.revision ?? 0),
    status: projected.status ?? '',
    errorCode: projected.errorCode ?? '',
    projection
  }
}

function decodeTeamCoordinator(
  projected: evohime.desktop.v1.ITeamCoordinatorEvent | null | undefined
): TeamCoordinatorProjection | null {
  if (!projected) return null
  const raw = decodePayload(projected.projectionJson)
  let projection: unknown = null
  try { projection = JSON.parse(raw) } catch { projection = null }
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    workItemId: projected.workItemId ?? '',
    operation: projected.operation ?? '',
    revision: Number(projected.revision ?? 0),
    status: projected.status ?? '',
    errorCode: projected.errorCode ?? '',
    projection
  }
}

function decodeProjectInstructionStack(
  projected: evohime.desktop.v1.IProjectInstructionStackEvent | null | undefined
): ProjectInstructionStackProjection | null {
  if (!projected) return null
  const raw = decodePayload(projected.projectionJson)
  let projection: unknown = null
  try { projection = JSON.parse(raw) } catch { projection = null }
  return { schemaVersion: Number(projected.schemaVersion ?? 0), workspaceRoot: projected.workspaceRoot ?? '', operation: projected.operation ?? '', revision: Number(projected.revision ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection }
}

function decodeWorkspaceSets(
  projected: evohime.desktop.v1.IWorkspaceSetsEvent | null | undefined
): WorkspaceSetsProjection | null {
  if (!projected) return null
  const raw = decodePayload(projected.projectionJson)
  let projection: unknown = null
  try { projection = JSON.parse(raw) } catch { projection = null }
  return { schemaVersion: Number(projected.schemaVersion ?? 0), setId: projected.setId ?? '', operation: projected.operation ?? '', version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection }
}

function decodeKnowledgeSourceRegistry(projected: evohime.desktop.v1.IKnowledgeSourceRegistryProjectRoleEvent | null | undefined): KnowledgeSourceRegistryProjection | null {
  if (!projected) return null
  const raw = decodePayload(projected.projectionJson)
  let projection: unknown = null
  try { projection = JSON.parse(raw) } catch { projection = null }
  return { schemaVersion: Number(projected.schemaVersion ?? 0), sourceId: projected.sourceId ?? '', operation: projected.operation ?? '', version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection }
}

function decodeAgentGitChangeSets(projected: evohime.desktop.v1.IAgentGitChangeSetsEvent | null | undefined): AgentGitChangeSetsProjection | null {
  if (!projected) return null
  let projection: unknown = null
  try { projection = JSON.parse(Buffer.from(projected.projectionJson ?? new Uint8Array()).toString('utf8')) } catch { projection = null }
  return { schemaVersion: projected.schemaVersion ?? 1, changeSetId: projected.changeSetId ?? '', operation: projected.operation ?? '', version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection }
}

function decodeArchitectEditorPipeline(projected: evohime.desktop.v1.IArchitectEditorModelPipelineEvent | null | undefined): ArchitectEditorPipelineProjection | null { if (!projected) return null; let projection: unknown = null; try { projection = JSON.parse(Buffer.from(projected.projectionJson ?? new Uint8Array()).toString('utf8')) } catch { projection = null }; return { schemaVersion: projected.schemaVersion ?? 1, pipelineId: projected.pipelineId ?? '', operation: projected.operation ?? '', version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection } }
function decodeEventVisualizerRegistry(projected: evohime.desktop.v1.IEventVisualizerRegistryEvent | null | undefined): EventVisualizerRegistryProjection | null { if (!projected) return null; let projection: unknown = null; try { projection = JSON.parse(Buffer.from(projected.projectionJson ?? new Uint8Array()).toString('utf8')) } catch { projection = null }; return { schemaVersion: projected.schemaVersion ?? 1, visualizerId: projected.visualizerId ?? '', operation: projected.operation ?? '', version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection } }
function decodeReasoningOperatorLibrary(projected: evohime.desktop.v1.IReasoningOperatorLibraryEvent | null | undefined): ReasoningOperatorLibraryProjection | null { if (!projected) return null; let projection: unknown = null; try { projection = JSON.parse(Buffer.from(projected.projectionJson ?? new Uint8Array()).toString('utf8')) } catch { projection = null }; return { schemaVersion: projected.schemaVersion ?? 1, operatorId: projected.operatorId ?? '', operation: projected.operation ?? '', version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '', projection } }

function decodeConversationEvent(
  projected: evohime.desktop.v1.IConversationEventProjection
): ConversationEventProjection {
  const raw = decodePayload(projected.payloadJson)
  let payload: unknown = null
  if (raw.length > 0) {
    try {
      payload = JSON.parse(raw) as unknown
    } catch {
      payload = { redacted: true, reason: 'malformed_renderer_projection' }
    }
  }
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    conversationId: projected.conversationId ?? '',
    eventId: projected.eventId ?? '',
    sequence: Number(projected.sequence ?? 0),
    timestampMs: Number(projected.timestampMs ?? 0),
    kind: projected.kind ?? '',
    category: projected.category ?? '',
    payload,
    correlationId: projected.correlationId ?? '',
    causationId: projected.causationId ?? '',
    taskId: projected.taskId ?? '',
    runId: projected.runId ?? '',
    turnId: projected.turnId ?? '',
    clientMessageId: projected.clientMessageId ?? '',
    persistenceClass: projected.persistenceClass ?? '',
    sensitivity: projected.sensitivity ?? ''
  }
}

/**
 * Reads `resync.end`'s bounded JSON payload (`{more_available, latest_sequence}`,
 * emitted by Core alongside every `resync.end`/`replay.end` marker). A missing
 * or malformed payload is treated as "nothing more to fetch" rather than
 * risking a resync loop against an older Core that predates this field.
 */
function parseResyncEnd(event: evohime.desktop.v1.EventEnvelope): { moreAvailable: boolean } {
  const raw = decodePayload(event.payload)
  if (!raw) {
    return { moreAvailable: false }
  }
  try {
    const parsed: unknown = JSON.parse(raw)
    const moreAvailable =
      typeof parsed === 'object' && parsed !== null && 'more_available' in parsed
        ? Boolean((parsed as { more_available: unknown }).more_available)
        : false
    return { moreAvailable }
  } catch {
    return { moreAvailable: false }
  }
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

function decodeTaskCheckpoint(
  projected: evohime.desktop.v1.ITaskCheckpointProjection | null | undefined
): TaskCheckpointProjection | null {
  if (!projected || !projected.taskId) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    checkpointId: projected.checkpointId ?? '',
    taskId: projected.taskId,
    workspaceId: projected.workspaceId ?? '',
    parentCheckpointId: projected.parentCheckpointId ?? '',
    status: projected.status ?? '',
    sourceEventSeq: Number(projected.sourceEventSeq ?? 0),
    createdAt: Number(projected.createdAt ?? 0),
    completedCount: Number(projected.completedCount ?? 0),
    remainingCount: Number(projected.remainingCount ?? 0),
    blockerCount: Number(projected.blockerCount ?? 0),
    blockers: [...(projected.blockers ?? [])],
    refs: (projected.refs ?? []).map(decodeTaskCheckpointRef),
    recoveryDisposition: projected.recoveryDisposition ?? 'blocked',
    recoveryWarning: projected.recoveryWarning ?? '',
    replayedEventTypes: [...(projected.replayedEventTypes ?? [])],
    canRequestResume: Boolean(projected.canRequestResume),
    replayedEventCount: Number(projected.replayedEventCount ?? 0),
    policyId: projected.policyId ?? '',
    errorCode: projected.errorCode ?? ''
  }
}

function decodeAnalysisKernel(
  projected: evohime.desktop.v1.IAnalysisKernelProjection | null | undefined
): AnalysisKernelProjection | null {
  if (!projected || !projected.kernelId) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    kernelId: projected.kernelId,
    taskId: projected.taskId ?? '',
    workspaceId: projected.workspaceId ?? '',
    runtimeVersion: projected.runtimeVersion ?? '',
    packageManifestHash: projected.packageManifestHash ?? '',
    policyHash: projected.policyHash ?? '',
    status: projected.status ?? '',
    revision: Number(projected.revision ?? 0),
    limitsJson: decodePayload(projected.limitsJson),
    objectCount: Number(projected.objectCount ?? 0),
    truncated: Boolean(projected.truncated),
    errorCode: projected.errorCode ?? ''
  }
}

function decodeAnalysisKernelResult(
  projected: evohime.desktop.v1.IAnalysisKernelResult | null | undefined
): AnalysisKernelResult | null {
  if (!projected || (!projected.requestId && !projected.errorClass)) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    requestId: projected.requestId ?? '',
    status: projected.status ?? '',
    inlineResult: decodePayload(projected.inlineResult),
    sensitivity: projected.sensitivity ?? '',
    provenance: projected.provenance ?? '',
    errorClass: projected.errorClass ?? ''
  }
}

function decodeRefinement(projected: evohime.desktop.v1.IRefinementProjection | null | undefined): RefinementProjection | null {
  if (!projected || (!projected.candidateId && !projected.errorCode)) return null
  return { schemaVersion: Number(projected.schemaVersion ?? 0), candidateId: projected.candidateId ?? '', revision: Number(projected.revision ?? 0), ownerScope: projected.ownerScope ?? '', kind: projected.kind ?? '', target: projected.target ?? '', status: projected.status ?? '', patternKey: projected.patternKey ?? '', title: projected.title ?? '', evidenceCount: Number(projected.evidenceCount ?? 0), conflictCount: Number(projected.conflictCount ?? 0), confidence: Number(projected.confidence ?? 0), contentHash: projected.contentHash ?? '', policySnapshotHash: projected.policySnapshotHash ?? '', version: Number(projected.version ?? 0), errorCode: projected.errorCode ?? '', updatedAtMs: Number(projected.updatedAtMs ?? 0) }
}

function decodeRefinementList(projected: evohime.desktop.v1.IRefinementListProjection | null | undefined): { candidates: readonly RefinementProjection[]; truncated: boolean; errorCode: string } | null {
  if (!projected) return null
  return { candidates: (projected.candidates ?? []).map((candidate) => decodeRefinement(candidate)).filter((candidate): candidate is RefinementProjection => candidate !== null), truncated: Boolean(projected.truncated), errorCode: projected.errorCode ?? '' }
}

function decodeRefinementAction(projected: evohime.desktop.v1.IRefinementActionResult | null | undefined): RefinementActionResult | null {
  if (!projected || (!projected.candidateId && !projected.errorCode)) return null
  return { schemaVersion: Number(projected.schemaVersion ?? 0), candidateId: projected.candidateId ?? '', revision: Number(projected.revision ?? 0), action: projected.action ?? '', applied: Boolean(projected.applied), deduplicated: Boolean(projected.deduplicated), version: Number(projected.version ?? 0), status: projected.status ?? '', errorCode: projected.errorCode ?? '' }
}

function decodeTaskCheckpointRef(
  reference: evohime.desktop.v1.ITaskCheckpointRef
): TaskCheckpointRef {
  return {
    kind: reference.kind ?? '',
    id: reference.id ?? '',
    contentHash: reference.contentHash ?? '',
    sensitivity: reference.sensitivity ?? ''
  }
}

function decodeTaskCheckpointAction(
  projected: evohime.desktop.v1.ITaskCheckpointActionResult | null | undefined
): TaskCheckpointActionResult | null {
  if (!projected || !projected.taskId) return null
  return {
    taskId: projected.taskId,
    checkpointId: projected.checkpointId ?? '',
    action: projected.action ?? '',
    applied: Boolean(projected.applied),
    deduplicated: Boolean(projected.deduplicated),
    errorCode: projected.errorCode ?? '',
    errorMessage: projected.errorMessage ?? ''
  }
}

function decodeSkillCatalog(
  projected: evohime.desktop.v1.ISkillCatalogProjection | null | undefined
): SkillCatalog | null {
  if (!projected) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    skills: (projected.skills ?? []).map(decodeSkillMetadata),
    diagnostics: (projected.diagnostics ?? []).map(decodeSkillDiagnostic)
  }
}

function decodeSkillMetadata(projected: evohime.desktop.v1.ISkillMetadataProjection): SkillMetadata {
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    skillId: projected.skillId ?? '',
    name: projected.name ?? '',
    description: projected.description ?? '',
    version: projected.version ?? '',
    scope: projected.scope ?? '',
    sourceKind: projected.sourceKind ?? '',
    sourceRef: projected.sourceRef ?? '',
    contentHash: projected.contentHash ?? '',
    allowedTools: [...(projected.allowedTools ?? [])],
    requiredCapabilities: [...(projected.requiredCapabilities ?? [])],
    disableModelInvocation: Boolean(projected.disableModelInvocation),
    referenceCount: Number(projected.referenceCount ?? 0),
    validationStatus: projected.validationStatus ?? '',
    validationErrorCode: projected.validationErrorCode ?? '',
    warnings: [...(projected.warnings ?? [])],
    trustDecision: projected.trustDecision ?? 'scanning',
    riskClass: projected.riskClass ?? 'blocked',
    findingsCount: Number(projected.findingsCount ?? 0)
  }
}

function decodeSkillDiagnostic(projected: evohime.desktop.v1.ISkillDiagnosticProjection): SkillDiagnostic {
  return {
    code: projected.code ?? '',
    skillId: projected.skillId ?? '',
    sourceKind: projected.sourceKind ?? '',
    sourceRef: projected.sourceRef ?? '',
    message: projected.message ?? ''
  }
}

function decodeSkillContent(
  projected: evohime.desktop.v1.ISkillContentResult | null | undefined
): SkillContentResult | null {
  if (!projected || !projected.skillId) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    skillId: projected.skillId,
    version: projected.version ?? '',
    content: projected.content ?? '',
    contentHash: projected.contentHash ?? '',
    sourceRef: projected.sourceRef ?? '',
    errorCode: projected.errorCode ?? '',
    errorMessage: projected.errorMessage ?? '',
    cacheHit: Boolean(projected.cacheHit)
  }
}

function decodeSkillReference(
  projected: evohime.desktop.v1.ISkillReferenceResult | null | undefined
): SkillReferenceResult | null {
  if (!projected || !projected.skillId) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    skillId: projected.skillId,
    reference: projected.reference ?? '',
    content: projected.content ?? '',
    contentHash: projected.contentHash ?? '',
    sourceRef: projected.sourceRef ?? '',
    errorCode: projected.errorCode ?? '',
    errorMessage: projected.errorMessage ?? ''
  }
}

function decodeGoal(
  projected: evohime.desktop.v1.IGoalProjection | null | undefined
): GoalProjection | null {
  if (!projected || !projected.goalId) return null
  return decodeGoalProjection(projected)
}

function decodeGoalList(
  projected: evohime.desktop.v1.IGoalListProjection | null | undefined
): GoalListProjection | null {
  if (!projected) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    goals: (projected.goals ?? []).map(decodeGoalProjection),
    errorCode: projected.errorCode ?? '',
    truncated: projected.truncated ?? false
  }
}

function decodeGoalProjection(projected: evohime.desktop.v1.IGoalProjection): GoalProjection {
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    goalId: projected.goalId ?? '',
    version: Number(projected.version ?? 0),
    workspaceId: projected.workspaceId ?? '',
    chatId: projected.chatId ?? '',
    objective: projected.objective ?? '',
    successCriteria: (projected.successCriteria ?? []).map(decodeGoalCriterion),
    status: projected.status ?? '',
    progressSummary: projected.progressSummary ?? '',
    completedCriteria: [...(projected.completedCriteria ?? [])],
    remainingCriteria: [...(projected.remainingCriteria ?? [])],
    blockers: [...(projected.blockers ?? [])],
    nextAction: projected.nextAction ?? '',
    workflowRunIds: [...(projected.workflowRunIds ?? [])],
    childRunIds: [...(projected.childRunIds ?? [])],
    checkpointId: projected.checkpointId ?? '',
    tokenBudget: Number(projected.tokenBudget ?? 0),
    costBudgetMicros: Number(projected.costBudgetMicros ?? 0),
    continuationBudget: Number(projected.continuationBudget ?? 0),
    createdAtMs: Number(projected.createdAtMs ?? 0),
    updatedAtMs: Number(projected.updatedAtMs ?? 0),
    contentHash: projected.contentHash ?? '',
    recoveryWarning: projected.recoveryWarning ?? '',
    errorCode: projected.errorCode ?? ''
  }
}

function decodeGoalCriterion(projected: evohime.desktop.v1.IGoalCriterionProjection): GoalCriterion {
  return {
    id: projected.id ?? '',
    kind: projected.kind ?? '',
    statement: projected.statement ?? '',
    status: projected.status ?? '',
    evidenceRef: projected.evidenceRef ?? '',
    verifierId: projected.verifierId ?? '',
    verifierVersion: projected.verifierVersion ?? '',
    verifiedAtMs: Number(projected.verifiedAtMs ?? 0),
    provenance: projected.provenance ?? ''
  }
}

function decodeGoalAction(
  projected: evohime.desktop.v1.IGoalActionResult | null | undefined
): GoalActionResult | null {
  if (!projected || !projected.goalId) return null
  return {
    schemaVersion: Number(projected.schemaVersion ?? 0),
    goalId: projected.goalId,
    action: projected.action ?? '',
    applied: Boolean(projected.applied),
    deduplicated: Boolean(projected.deduplicated),
    goalVersion: Number(projected.goalVersion ?? 0),
    errorCode: projected.errorCode ?? '',
    errorMessage: projected.errorMessage ?? '',
    sequenceId: Number(projected.sequenceId ?? 0),
    goal: projected.goal ? decodeGoalProjection(projected.goal) : null
  }
}
