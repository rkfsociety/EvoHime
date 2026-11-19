import { describe, expect, it } from 'vitest'

import { prependCoreEvents } from '../src/renderer/src/core-event-buffer'
import type { CoreEvent } from '../src/shared/api'

function event(sequenceId: number, profile: string, eventType = 'hive.model.catalog'): CoreEvent {
  return {
    sequenceId,
    coreInstanceId: 'core-1',
    sessionEpoch: 1,
    taskId: '',
    eventType,
    payload: JSON.stringify({ profile }),
    executionEvent: null
  }
}

describe('Core event buffer', () => {
  it('keeps successive transient catalog responses so the newest provider wins', () => {
    const literouterCatalog = event(0, 'literouter')
    const routCatalog = event(0, 'rout')

    const events = prependCoreEvents([literouterCatalog], [routCatalog], 100)

    expect(events).toEqual([routCatalog, literouterCatalog])
    expect(JSON.parse(events[0]!.payload)).toEqual({ profile: 'rout' })
  })

  it('still deduplicates replayed persisted events', () => {
    const replay = event(42, 'rout')

    expect(prependCoreEvents([replay], [replay], 100)).toEqual([replay])
  })
})
