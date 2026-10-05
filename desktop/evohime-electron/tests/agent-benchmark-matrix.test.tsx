// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { AgentBenchmarkMatrixPanel } from '../src/renderer/src/AgentBenchmarkMatrixPanel'

describe('Agent Benchmark Matrix projection', () => {
  afterEach(cleanup)

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

  it('sends an explicit baseline approval bound to the report and job revision', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke } } })
    render(<AgentBenchmarkMatrixPanel />)
    const values: Record<string, string> = {
      'ID adaptation job / run': 'adapt-job-1',
      'Challenge ID': 'challenge-1',
      'Model profile ID': 'local-model',
      'Agent profile ID': 'agent-1',
      'SHA-256 отчёта': 'a'.repeat(64),
      'Revision завершённой adaptation job': '7'
    }
    for (const [label, value] of Object.entries(values)) {
      fireEvent.change(screen.getByLabelText(label), { target: { value } })
    }
    fireEvent.click(screen.getByRole('button', { name: 'Явно утвердить baseline' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('benchmarkMatrix.approveBaseline', expect.objectContaining({
      runId: 'adapt-job-1', challengeId: 'challenge-1', modelProfileId: 'local-model',
      agentProfileId: 'agent-1', reportSha256: 'a'.repeat(64), expectedVersion: 7,
      idempotencyKey: expect.any(String)
    })))
  })
})
