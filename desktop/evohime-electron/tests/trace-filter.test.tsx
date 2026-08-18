import { describe, expect, it } from 'vitest'

import type { CoreEvent } from '../src/shared/api'
import { filterEventsForChat } from '../src/renderer/src/trace-filter'

function event(taskId: string, sequenceId: number): CoreEvent {
  return { taskId, sequenceId, eventType: 'task.started', payload: '{}' }
}

describe('filterEventsForChat', () => {
  it('returns only task events belonging to the selected chat', () => {
    const events = [event('task-a', 1), event('', 2), event('task-b', 3), event('task-a', 4)]

    expect(filterEventsForChat(events, { taskIds: ['task-a'] })).toEqual([events[0], events[3]])
  })

  it('does not expose the global event stream without a selected chat', () => {
    expect(filterEventsForChat([event('task-a', 1)], null)).toEqual([])
  })
})
