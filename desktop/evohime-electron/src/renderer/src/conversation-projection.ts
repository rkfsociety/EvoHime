import type { ConversationEventProjection, CoreEvent } from '@shared/api'

const MAX_LIVE_EVENTS = 400
const coreEventProjectionCache = new WeakMap<ConversationEventProjection, CoreEvent | null>()
const conversationEventsCache = new WeakMap<ConversationProjectionState, readonly ConversationEventProjection[]>()

export interface OptimisticConversationMessage {
  readonly clientMessageId: string
  readonly taskId: string
  readonly content: string
  readonly status: 'sending' | 'retry' | 'failed'
}

export type ConversationSync =
  | { readonly state: 'complete' }
  | { readonly state: 'gap'; readonly expectedSequence: number; readonly receivedSequence: number }
  | { readonly state: 'conflict'; readonly sequence: number }
  | { readonly state: 'cursor-expired'; readonly earliestAvailableSequence: number }

export interface ConversationProjectionState {
  readonly conversationId: string
  readonly cacheKey: string
  /** Pages explicitly loaded by the user or by the initial history request. */
  readonly historyEvents: readonly ConversationEventProjection[]
  /** Events received from the current subscription/catch-up stream. */
  readonly liveEvents: readonly ConversationEventProjection[]
  readonly optimistic: readonly OptimisticConversationMessage[]
  readonly lastSequence: number
  /** True after the initial history establishes this conversation's baseline. */
  readonly historyReady: boolean
  readonly sync: ConversationSync
}

export function createConversationProjection(conversationId: string, cacheKey = ''): ConversationProjectionState {
  return {
    conversationId,
    cacheKey,
    historyEvents: [],
    liveEvents: [],
    optimistic: [],
    lastSequence: 0,
    historyReady: false,
    sync: { state: 'complete' }
  }
}

export function conversationEvents(
  state: ConversationProjectionState
): readonly ConversationEventProjection[] {
  const cached = conversationEventsCache.get(state)
  if (cached) return cached
  const merged = mergeSortedEvents(state.historyEvents, state.liveEvents)
  conversationEventsCache.set(state, merged)
  return merged
}

function mergeSortedEvents<T extends { readonly sequence: number }>(
  left: readonly T[],
  right: readonly T[]
): T[] {
  const merged = new Array<T>(left.length + right.length)
  let leftIndex = 0
  let rightIndex = 0
  let outputIndex = 0
  while (leftIndex < left.length && rightIndex < right.length) {
    if ((left[leftIndex]?.sequence ?? 0) <= (right[rightIndex]?.sequence ?? 0)) {
      merged[outputIndex++] = left[leftIndex++]!
    } else {
      merged[outputIndex++] = right[rightIndex++]!
    }
  }
  while (leftIndex < left.length) merged[outputIndex++] = left[leftIndex++]!
  while (rightIndex < right.length) merged[outputIndex++] = right[rightIndex++]!
  return merged
}

function indexConversationEvents(state: ConversationProjectionState): {
  readonly byId: Map<string, ConversationEventProjection>
  readonly bySequence: Map<number, ConversationEventProjection>
  readonly historyIds: Set<string>
} {
  const byId = new Map<string, ConversationEventProjection>()
  const bySequence = new Map<number, ConversationEventProjection>()
  const historyIds = new Set<string>()
  for (const event of state.historyEvents) {
    if (!byId.has(event.eventId)) byId.set(event.eventId, event)
    if (!bySequence.has(event.sequence)) bySequence.set(event.sequence, event)
    historyIds.add(event.eventId)
  }
  for (const event of state.liveEvents) {
    if (!byId.has(event.eventId)) byId.set(event.eventId, event)
    if (!bySequence.has(event.sequence)) bySequence.set(event.sequence, event)
  }
  return { byId, bySequence, historyIds }
}

export function addOptimisticMessage(
  state: ConversationProjectionState,
  message: OptimisticConversationMessage
): ConversationProjectionState {
  return {
    ...state,
    optimistic: [...state.optimistic.filter((item) => item.clientMessageId !== message.clientMessageId), message]
  }
}

export function markOptimisticFailed(
  state: ConversationProjectionState,
  clientMessageId: string
): ConversationProjectionState {
  return {
    ...state,
    optimistic: state.optimistic.map((message) =>
      message.clientMessageId === clientMessageId ? { ...message, status: 'failed' } : message
    )
  }
}

export function markOptimisticRetry(
  state: ConversationProjectionState,
  clientMessageId: string
): ConversationProjectionState {
  return {
    ...state,
    optimistic: state.optimistic.map((message) =>
      message.clientMessageId === clientMessageId ? { ...message, status: 'retry' } : message
    )
  }
}

export function resumeAtRetainedBoundary(
  state: ConversationProjectionState,
  earliestAvailableSequence: number
): ConversationProjectionState {
  return {
    ...state,
    historyEvents: [],
    liveEvents: [],
    lastSequence: Math.max(0, earliestAvailableSequence - 1),
    historyReady: true,
    sync: { state: 'complete' }
  }
}

export function applyConversationEvents(
  state: ConversationProjectionState,
  incoming: readonly ConversationEventProjection[]
): ConversationProjectionState {
  const indexed = indexConversationEvents(state)
  let liveEvents: ConversationEventProjection[] | null = null
  let optimistic = state.optimistic
  let lastSequence = state.lastSequence
  let historyReady = state.historyReady
  let sync = state.sync
  for (const event of [...incoming].sort((left, right) => left.sequence - right.sequence)) {
    if (event.conversationId !== state.conversationId || event.schemaVersion !== 1) continue
    const byId = indexed.byId.get(event.eventId)
    if (byId) {
      if (!sameConversationEvent(byId, event)) {
        sync = { state: 'conflict', sequence: event.sequence }
      }
      optimistic = reconcileOptimisticMessages(optimistic, event)
      continue
    }
    const atSequence = indexed.bySequence.get(event.sequence)
    if (atSequence) {
      sync = { state: 'conflict', sequence: event.sequence }
      continue
    }
    if (event.sequence <= lastSequence) continue
    const expected = lastSequence + 1
    if (event.sequence !== expected) {
      sync = { state: 'gap', expectedSequence: expected, receivedSequence: event.sequence }
      continue
    }
    liveEvents ??= [...state.liveEvents]
    liveEvents.push(event)
    indexed.byId.set(event.eventId, event)
    indexed.bySequence.set(event.sequence, event)
    if (liveEvents.length > MAX_LIVE_EVENTS) {
      const removed = liveEvents.shift()
      if (removed && !indexed.historyIds.has(removed.eventId)) {
        if (indexed.byId.get(removed.eventId) === removed) indexed.byId.delete(removed.eventId)
        if (indexed.bySequence.get(removed.sequence) === removed) indexed.bySequence.delete(removed.sequence)
      }
    }
    optimistic = reconcileOptimisticMessages(optimistic, event)
    historyReady = historyReady || event.sequence === 1
    lastSequence = event.sequence
    sync = { state: 'complete' }
  }
  const nextLiveEvents = liveEvents ?? state.liveEvents
  return nextLiveEvents === state.liveEvents
    && optimistic === state.optimistic
    && lastSequence === state.lastSequence
    && historyReady === state.historyReady
    && sync === state.sync
    ? state
    : { ...state, liveEvents: nextLiveEvents, optimistic, lastSequence, historyReady, sync }
}

/** Adds a history page without moving the live cursor. */
export function prependConversationEvents(
  state: ConversationProjectionState,
  incoming: readonly ConversationEventProjection[]
): ConversationProjectionState {
  const indexed = indexConversationEvents(state)
  const additions: ConversationEventProjection[] = []
  let optimistic = state.optimistic
  let sync = state.sync
  const older = incoming
    .filter((event) => event.conversationId === state.conversationId && event.schemaVersion === 1)
    .sort((left, right) => left.sequence - right.sequence)
  for (const event of older) {
    const byId = indexed.byId.get(event.eventId)
    if (byId) {
      if (!sameConversationEvent(byId, event)) {
        sync = { state: 'conflict', sequence: event.sequence }
      }
      optimistic = reconcileOptimisticMessages(optimistic, event)
      continue
    }
    const atSequence = indexed.bySequence.get(event.sequence)
    if (atSequence) {
      if (!sameConversationEvent(atSequence, event)) {
        sync = { state: 'conflict', sequence: event.sequence }
      }
      optimistic = reconcileOptimisticMessages(optimistic, event)
      continue
    }
    additions.push(event)
    indexed.byId.set(event.eventId, event)
    indexed.bySequence.set(event.sequence, event)
    optimistic = reconcileOptimisticMessages(optimistic, event)
  }
  if (additions.length === 0 && optimistic === state.optimistic && sync === state.sync) return state
  return {
    ...state,
    historyEvents: additions.length === 0 ? state.historyEvents : mergeSortedEvents(state.historyEvents, additions),
    optimistic,
    sync
  }
}

function reconcileOptimisticMessages(
  optimistic: readonly OptimisticConversationMessage[],
  event: ConversationEventProjection
): readonly OptimisticConversationMessage[] {
  if (event.kind !== 'user_message_accepted' || event.clientMessageId.length === 0
    || !optimistic.some((message) => message.clientMessageId === event.clientMessageId)) return optimistic
  return optimistic.filter((message) => message.clientMessageId !== event.clientMessageId)
}

/** Adds the first history page and advances the live cursor to its newest event. */
export function applyInitialConversationHistory(
  state: ConversationProjectionState,
  incoming: readonly ConversationEventProjection[]
): ConversationProjectionState {
  const next = prependConversationEvents(state, incoming)
  const hasLiveCursor = state.historyEvents.length > 0 || state.liveEvents.length > 0
  let newestSequence = hasLiveCursor ? next.lastSequence : 0
  if (!hasLiveCursor) {
    for (const event of next.historyEvents) newestSequence = Math.max(newestSequence, event.sequence)
  }
  const sync = next.sync.state === 'gap' && coversGap(conversationEvents(next), next.sync)
    ? { state: 'complete' as const }
    : next.sync
  return { ...next, lastSequence: newestSequence, historyReady: true, sync }
}

function coversGap(
  events: readonly ConversationEventProjection[],
  gap: Extract<ConversationSync, { readonly state: 'gap' }>
): boolean {
  let expected = gap.expectedSequence
  for (const event of events) {
    if (event.sequence < gap.expectedSequence) continue
    if (event.sequence > gap.receivedSequence) break
    if (event.sequence !== expected) return false
    if (expected === gap.receivedSequence) return true
    expected += 1
  }
  return false
}

function sameConversationEvent(
  left: ConversationEventProjection,
  right: ConversationEventProjection
): boolean {
  return left.schemaVersion === right.schemaVersion
    && left.conversationId === right.conversationId
    && left.eventId === right.eventId
    && left.sequence === right.sequence
    && left.timestampMs === right.timestampMs
    && left.kind === right.kind
    && left.category === right.category
    && JSON.stringify(left.payload) === JSON.stringify(right.payload)
    && left.correlationId === right.correlationId
    && left.causationId === right.causationId
    && left.taskId === right.taskId
    && left.runId === right.runId
    && left.turnId === right.turnId
    && left.clientMessageId === right.clientMessageId
    && left.persistenceClass === right.persistenceClass
    && left.sensitivity === right.sensitivity
}

export interface BatchedAssistantMessage {
  readonly kind: 'stream' | 'finalized' | 'failed'
  readonly taskId: string
  readonly content: string
  readonly firstSequence: number
  readonly lastSequence: number
}

export function batchAssistantDeltas(
  events: readonly ConversationEventProjection[]
): readonly BatchedAssistantMessage[] {
  const output: BatchedAssistantMessage[] = []
  for (const event of events) {
    const content = textPayload(event.payload, event.kind === 'assistant_message_finalized' ? 'content' : 'content')
    if (event.kind === 'assistant_message_delta') {
      const last = output.at(-1)
      if (last?.kind === 'stream' && last.taskId === event.taskId) {
        output[output.length - 1] = {
          ...last,
          content: `${last.content}${content}`,
          lastSequence: event.sequence
        }
      } else {
        output.push({ kind: 'stream', taskId: event.taskId, content, firstSequence: event.sequence, lastSequence: event.sequence })
      }
    } else if (event.kind === 'assistant_message_finalized') {
      output.push({ kind: 'finalized', taskId: event.taskId, content, firstSequence: event.sequence, lastSequence: event.sequence })
    } else if (event.kind === 'assistant_message_failed') {
      output.push({
        kind: 'failed',
        taskId: event.taskId,
        content: textPayload(event.payload, 'error_code') || 'task_failed',
        firstSequence: event.sequence,
        lastSequence: event.sequence
      })
    }
  }
  return output
}

export interface UsageProjection {
  readonly inputTokens: number
  readonly outputTokens: number
  readonly bySource: Readonly<Record<string, number>>
}

export function projectUsage(events: readonly ConversationEventProjection[]): UsageProjection {
  let inputTokens = 0
  let outputTokens = 0
  const bySource: Record<string, number> = {}
  for (const event of events) {
    if (event.kind !== 'usage_snapshot' || typeof event.payload !== 'object' || event.payload === null) continue
    const value = event.payload as Record<string, unknown>
    const input = finite(value['input_tokens'])
    const output = finite(value['output_tokens'])
    const source = typeof value['source'] === 'string' && value['source'].length > 0 ? value['source'] : 'unknown'
    inputTokens += input
    outputTokens += output
    bySource[source] = (bySource[source] ?? 0) + input + output
  }
  return { inputTokens, outputTokens, bySource }
}

/** Compatibility projection for existing transcript components. */
export function conversationEventsToCoreEvents(
  events: readonly ConversationEventProjection[]
): readonly CoreEvent[] {
  const output: CoreEvent[] = []
  for (const event of events) {
    let projected = coreEventProjectionCache.get(event)
    if (projected === undefined) {
      const eventType = coreEventType(event.kind)
      if (eventType === null) {
        projected = null
      } else {
        const payload = event.kind === 'assistant_message_finalized'
          ? { TaskCompleted: { task_id: event.taskId, final_message: textPayload(event.payload, 'content') } }
          : event.kind === 'assistant_message_delta'
            ? { AssistantDelta: { task_id: event.taskId, content: textPayload(event.payload, 'content') } }
            : event.kind === 'usage_snapshot'
              ? {
                  ModelContext: {
                    ...(event.payload as Record<string, unknown>),
                    estimated_tokens: (event.payload as Record<string, unknown>)['input_tokens'] ?? 0
                  }
                }
              : event.payload
        projected = {
          sequenceId: event.sequence,
          taskId: event.taskId,
          eventType,
          payload: JSON.stringify(payload),
          executionEvent: null,
          conversationEventLog: null
        }
      }
      // Projection objects are immutable snapshots from the Core IPC boundary.
      // Weak keys avoid retaining histories after a conversation is closed.
      coreEventProjectionCache.set(event, projected)
    }
    if (projected !== null) output.push(projected)
  }
  return output
}

function coreEventType(kind: string): string | null {
  const mapping: Record<string, string> = {
    assistant_message_delta: 'agent.message.delta',
    assistant_message_finalized: 'task.completed',
    assistant_message_failed: 'task.failed',
    tool_started: 'tool.started',
    tool_output: 'tool.output',
    approval_required: 'approval.required',
    task_started: 'task.started',
    task_stopped: 'task.stopped',
    usage_snapshot: 'model.context',
    file_activity: 'storage.progress',
    backend_snapshot: 'routing.terminal',
    child_run_summary: 'child.workflow',
    recovery_snapshot: 'task.recovery',
    goal_snapshot: 'goal.updated',
    artifact_snapshot: 'artifact.saved',
    task_progress: 'workflow.progress'
  }
  return mapping[kind] ?? null
}

function textPayload(payload: unknown, key: string): string {
  if (typeof payload !== 'object' || payload === null) return ''
  const value = (payload as Record<string, unknown>)[key]
  return typeof value === 'string' ? value : ''
}

function finite(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0
}
