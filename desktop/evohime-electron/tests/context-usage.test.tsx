// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { CoreEvent } from '../src/shared/api'
import { ContextUsage } from '../src/renderer/src/ContextUsage'

function event(sequenceId: number, payload: unknown): CoreEvent {
  return {
    sequenceId,
    taskId: 'task-1',
    eventType: 'model.context',
    payload: JSON.stringify(payload)
  }
}

describe('context usage', () => {
  it('reads the wrapped ModelContext payload emitted by Core', () => {
    render(
      <ContextUsage
        events={[event(1, { ModelContext: { estimated_tokens: 12_345, context_limit_tokens: 100_000 } })]}
      />
    )

    expect(screen.getByRole('status').getAttribute('aria-label')).toContain('Текущий контекст: 12%')
    expect(screen.getByRole('status').getAttribute('aria-label')).toContain('12')
    expect(screen.getByText('12')).toBeTruthy()
  })

  it('keeps accepting the flat payload shape', () => {
    render(
      <ContextUsage
        events={[event(1, { estimated_tokens: 25, context_limit_tokens: 100 })]}
      />
    )

    expect(screen.getByText('25')).toBeTruthy()
  })
})
