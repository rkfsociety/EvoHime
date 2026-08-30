// @vitest-environment jsdom
import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { EventTriggerRuntimePanel } from '../src/renderer/src/EventTriggerRuntimePanel'

describe('Event Trigger Runtime projection', () => {
  it('reads Core metadata and exposes provider unavailability explicitly', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke } } })
    render(<EventTriggerRuntimePanel />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('eventTriggerRuntime.list', expect.any(Object)))
    expect(await screen.findByText(/webhook-провайдеры недоступны/)).toBeTruthy()
    expect(document.body.textContent).not.toContain('payload')
  })
})
