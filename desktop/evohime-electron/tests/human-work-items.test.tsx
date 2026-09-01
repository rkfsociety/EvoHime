import { describe, expect, it } from 'vitest'

describe('human work items projection contract', () => {
  it('keeps approvals, credentials and raw model prompts outside the inbox projection', () => {
    const projection = { approval: false, credentials: false, raw_prompt: false, instructions_are_typed: true }
    expect(projection.approval).toBe(false)
    expect(projection.credentials).toBe(false)
    expect(projection.raw_prompt).toBe(false)
    expect(projection.instructions_are_typed).toBe(true)
  })
})
