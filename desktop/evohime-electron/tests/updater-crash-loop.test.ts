import { describe, expect, it } from 'vitest'
import { clearUpdaterStart, recordUpdaterStart } from '../src/main/update/crash-loop'

describe('updater crash-loop guard', () => {
  it('blocks the third start in a bounded time window and resets after success', () => {
    const state = `${process.cwd()}/.tmp-updater-crash-loop-${Date.now()}`
    expect(recordUpdaterStart(state, 1).blocked).toBe(false)
    expect(recordUpdaterStart(state, 2).blocked).toBe(false)
    expect(recordUpdaterStart(state, 3).blocked).toBe(true)
    clearUpdaterStart(state)
    expect(recordUpdaterStart(state, 4).blocked).toBe(false)
    clearUpdaterStart(state)
  })

  it('starts a fresh window after the previous one expires', () => {
    const state = `${process.cwd()}/.tmp-updater-crash-loop-expired-${Date.now()}`
    recordUpdaterStart(state, 1)
    recordUpdaterStart(state, 2)
    expect(recordUpdaterStart(state, 10 * 60_000 + 1).attempts).toBe(1)
    clearUpdaterStart(state)
  })
})
