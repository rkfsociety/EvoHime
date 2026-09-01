// @vitest-environment jsdom
import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { CausalCollaborationBusPanel } from '../src/renderer/src/CausalCollaborationBusPanel'

describe('Causal Collaboration Bus projection', () => {
  it('запрашивает metadata-only список через Core', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<CausalCollaborationBusPanel />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('causalCollaborationBus.list', expect.any(Object)))
    expect(screen.getByText(/payload, prompts, credentials/i)).toBeTruthy()
  })
})
