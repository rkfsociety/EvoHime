import { describe, expect, it } from 'vitest'

import { LedgerEventDedup } from '../src/main/ipc/ledger-event-dedup'

describe('LedgerEventDedup', () => {
  it('evicts the oldest ID while retaining a bounded set', () => {
    const dedup = new LedgerEventDedup(2)

    expect(dedup.observe('one')).toBe(true)
    expect(dedup.observe('two')).toBe(true)
    expect(dedup.observe('one')).toBe(false)
    expect(dedup.observe('three')).toBe(true)
    expect(dedup.size).toBe(2)
    expect(dedup.observe('two')).toBe(false)
    expect(dedup.observe('one')).toBe(true)
  })

  it('does not retain IDs when configured with a zero limit', () => {
    const dedup = new LedgerEventDedup(0)

    expect(dedup.observe('one')).toBe(true)
    expect(dedup.observe('one')).toBe(true)
    expect(dedup.size).toBe(0)
  })
})
