// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { AgentBenchmarkMatrixPanel } from '../src/renderer/src/AgentBenchmarkMatrixPanel'

describe('Agent Benchmark Matrix projection', () => {
  it('uses Core for metadata and start, without exposing benchmark payloads', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke } } })
    render(<AgentBenchmarkMatrixPanel />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('benchmarkMatrix.list', expect.any(Object)))
    fireEvent.click(screen.getByRole('button', { name: /запустить deterministic/i }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('benchmarkMatrix.start', expect.objectContaining({ suiteId: 'core', attempts: 3 })))
    expect(document.body.textContent).not.toContain('prompt')
    expect(document.body.textContent).not.toContain('output')
  })
})
