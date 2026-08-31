import { describe, expect, it } from 'vitest'

import {
  addOptimisticMessage,
  applyConversationEvents,
  batchAssistantDeltas,
  createConversationProjection,
  projectUsage,
  resumeAtRetainedBoundary
} from '../src/renderer/src/conversation-projection'
import type { ConversationEventProjection } from '../src/shared/api'

function event(sequence: number, eventId: string, kind: string, payload: unknown = {}, clientMessageId = ''): ConversationEventProjection {
  return {
    schemaVersion: 1,
    conversationId: 'conversation-1',
    eventId,
    sequence,
    timestampMs: sequence,
    kind,
    category: kind === 'usage_snapshot' ? 'usage' : 'message',
    payload,
    correlationId: '',
    causationId: '',
    taskId: 'task-1',
    runId: 'task-1',
    turnId: 'task-1',
    clientMessageId,
    persistenceClass: kind === 'assistant_message_delta' ? 'transient_stream' : 'durable',
    sensitivity: 'internal'
  }
}

describe('conversation projection', () => {
  it('ignores exact duplicates and reports gaps and sequence conflicts', () => {
    let state = createConversationProjection('conversation-1')
    state = applyConversationEvents(state, [event(1, 'event-1', 'task_started')])
    expect(applyConversationEvents(state, [event(1, 'event-1', 'task_started')])).toEqual(state)
    expect(applyConversationEvents(state, [event(1, 'event-1', 'task_started', { changed: true })]).sync.state).toBe('conflict')
    expect(applyConversationEvents(state, [event(3, 'event-3', 'task_completed')]).sync).toEqual({
      state: 'gap', expectedSequence: 2, receivedSequence: 3
    })
    expect(applyConversationEvents(state, [event(1, 'corrupt', 'task_started')]).sync.state).toBe('conflict')
  })

  it('reconciles optimistic messages only by stable client id', () => {
    let state = addOptimisticMessage(
      createConversationProjection('conversation-1'),
      { clientMessageId: 'client-1', taskId: 'task-1', content: 'одинаково', status: 'sending' }
    )
    state = addOptimisticMessage(
      state,
      { clientMessageId: 'client-2', taskId: 'task-2', content: 'одинаково', status: 'sending' }
    )
    state = applyConversationEvents(state, [event(1, 'event-1', 'user_message_accepted', { content: 'одинаково' }, 'client-1')])
    expect(state.optimistic.map((message) => message.clientMessageId)).toEqual(['client-2'])
  })

  it('batches deltas in order, flushes on finalize and aggregates usage purposes', () => {
    const events = [
      event(1, 'd1', 'assistant_message_delta', { content: 'a' }),
      event(2, 'd2', 'assistant_message_delta', { content: 'b' }),
      event(3, 'f1', 'assistant_message_finalized', { content: 'ab' }),
      event(4, 'u1', 'usage_snapshot', { source: 'main_model', input_tokens: 4, output_tokens: 2 }),
      event(5, 'u2', 'usage_snapshot', { source: 'reviewer', input_tokens: 3, output_tokens: 1 })
    ]
    expect(batchAssistantDeltas(events).map((item) => item.content)).toEqual(['ab', 'ab'])
    expect(projectUsage(events)).toEqual({ inputTokens: 7, outputTokens: 3, bySource: { main_model: 6, reviewer: 4 } })
  })

  it('never applies events from another conversation', () => {
    const other = { ...event(1, 'foreign', 'task_started'), conversationId: 'conversation-2' }
    const state = applyConversationEvents(createConversationProjection('conversation-1'), [other])
    expect(state.events).toEqual([])
  })

  it('resumes from the earliest retained sequence after cursor expiry', () => {
    const expired = {
      ...createConversationProjection('conversation-1'),
      sync: { state: 'cursor-expired' as const, earliestAvailableSequence: 5 }
    }
    const resumed = applyConversationEvents(resumeAtRetainedBoundary(expired, 5), [event(5, 'event-5', 'task_started')])
    expect(resumed.lastSequence).toBe(5)
    expect(resumed.sync.state).toBe('complete')
  })

  it('bounds projected history and isolates Core generations in the cache key', () => {
    const first = createConversationProjection('conversation-1', 'core-a:1:1')
    const events = Array.from({ length: 600 }, (_, index) => event(index + 1, `event-${index + 1}`, 'task_progress'))
    const projected = applyConversationEvents(first, events)
    expect(projected.events).toHaveLength(400)
    expect(projected.lastSequence).toBe(600)
    expect(projected.cacheKey).toBe('core-a:1:1')
    expect(createConversationProjection('conversation-1', 'core-b:2:1').events).toEqual([])
  })
})
