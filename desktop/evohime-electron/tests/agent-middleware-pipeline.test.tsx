// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { AgentMiddlewarePipelinePanel } from '../src/renderer/src/AgentMiddlewarePipelinePanel'

describe('Agent Middleware Pipeline projection', () => {
  it('routes metadata and actions through Core only', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke } } })
    render(<AgentMiddlewarePipelinePanel />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('agentMiddleware.list', expect.any(Object)))
    fireEvent.click(screen.getByRole('button', { name: /проверить pipeline/i }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('agentMiddleware.start', expect.objectContaining({ ownerScope: 'middleware' })))
    expect(document.body.textContent).not.toMatch(/prompt|output|secret/i)
  })
})
