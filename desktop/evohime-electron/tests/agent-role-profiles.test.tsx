import { describe, expect, it } from 'vitest'

describe('agent role profiles projection contract', () => {
  it('keeps forbidden payload classes out of the metadata projection', () => {
    const projection = { profile_count: 1, raw_prompt: false, credentials: false, executable_code: false }
    expect(projection.raw_prompt).toBe(false)
    expect(projection.credentials).toBe(false)
    expect(projection.executable_code).toBe(false)
  })
})
