import type { ConversationEventProjection, CoreEvent } from '@shared/api'

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
  readonly events: readonly ConversationEventProjection[]
  readonly optimistic: readonly OptimisticConversationMessage[]
  readonly lastSequence: number
  readonly sync: ConversationSync
}

export function createConversationProjection(conversationId: string, cacheKey = ''): ConversationProjectionState {
  return { conversationId, cacheKey, events: [], optimistic: [], lastSequence: 0, sync: { state: 'complete' } }
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
    events: [],
    lastSequence: Math.max(0, earliestAvailableSequence - 1),
    sync: { state: 'complete' }
  }
}

export function applyConversationEvents(
  state: ConversationProjectionState,
  incoming: readonly ConversationEventProjection[]
): ConversationProjectionState {
  let next = state
  for (const event of [...incoming].sort((left, right) => left.sequence - right.sequence)) {
    if (event.conversationId !== next.conversationId || event.schemaVersion !== 1) continue
    const byId = next.events.find((known) => known.eventId === event.eventId)
    if (byId) {
      if (!sameConversationEvent(byId, event)) {
        next = { ...next, sync: { state: 'conflict', sequence: event.sequence } }
      }
      continue
    }
    const atSequence = next.events.find((known) => known.sequence === event.sequence)
    if (atSequence) {
      next = { ...next, sync: { state: 'conflict', sequence: event.sequence } }
      continue
    }
    if (event.sequence <= next.lastSequence) continue
    const expected = next.lastSequence + 1
    if (event.sequence !== expected) {
      next = {
        ...next,
        sync: { state: 'gap', expectedSequence: expected, receivedSequence: event.sequence }
      }
      continue
    }
    next = {
      ...next,
      events: [...next.events, event].slice(-400),
      optimistic:
        event.kind === 'user_message_accepted' && event.clientMessageId.length > 0
          ? next.optimistic.filter((message) => message.clientMessageId !== event.clientMessageId)
          : next.optimistic,
      lastSequence: event.sequence,
      sync: { state: 'complete' }
    }
  }
  return next
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
      output.push({ kind: 'failed', taskId: event.taskId, content: textPayload(event.payload, 'error'), firstSequence: event.sequence, lastSequence: event.sequence })
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
  return events.flatMap((event): CoreEvent[] => {
    const eventType = coreEventType(event.kind)
    if (eventType === null) return []
    const payload = event.kind === 'assistant_message_finalized'
      ? { TaskCompleted: { task_id: event.taskId, final_message: textPayload(event.payload, 'content') } }
      : event.kind === 'assistant_message_delta'
        ? { AssistantDelta: { task_id: event.taskId, content: textPayload(event.payload, 'content') } }
        : event.kind === 'usage_snapshot'
          ? { ModelContext: { ...(event.payload as Record<string, unknown>), estimated_tokens: (event.payload as Record<string, unknown>)['input_tokens'] ?? 0 } }
        : event.payload
    return [{
      sequenceId: event.sequence,
      taskId: event.taskId,
      eventType,
      payload: JSON.stringify(payload),
      executionEvent: null,
      conversationEventLog: null
    }]
  })
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
    usage_snapshot: 'model.context'
    ,file_activity: 'storage.progress',
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
