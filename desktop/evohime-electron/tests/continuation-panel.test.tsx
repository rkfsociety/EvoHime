/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import type { CoreEvent } from '../src/shared/api'
import { ContinuationPanel } from '../src/renderer/src/ContinuationPanel'

const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })

afterEach(() => {
  cleanup()
  invoke.mockClear()
  delete (window as unknown as { evohime?: unknown }).evohime
})

function installApi(): void {
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true }
  }
}

describe('ContinuationPanel', () => {
  it('показывает только Core projection и отправляет явную остановку', async () => {
    installApi()
    const event: CoreEvent = {
      sequenceId: 0,
      taskId: 'task-1',
      eventType: 'continuation.run',
      payload: '',
      executionEvent: null,
      continuation: {
        schemaVersion: 1,
        runId: 'run-1',
        ownerScope: 'owner-1',
        policyId: 'policy-1',
        policyRevision: 1,
        policyHash: 'hash-1',
        state: 'running',
        continuationIndex: 1,
        maxContinuations: 3,
        modelTurns: 1,
        maxModelTurns: 4,
        tokenUsed: 0,
        costUsedMicros: 0,
        stopReason: '',
        errorCode: '',
        gates: [{ gateId: 'gate-1', kind: 'tool', capabilityRef: 'git.diff', status: 'passed', evidenceRef: 'gate:gate-1', errorCode: '' }]
      }
    }
    render(<ContinuationPanel connection="connected" events={[event]} />)

    expect(screen.getByText('run-1')).toBeTruthy()
    expect(screen.getByText(/gate-1: passed/)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Остановить' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.stopContinuation'))).toBe(true)
    expect(screen.queryByText(/prompt|secret|reasoning/i)).toBeNull()
  })
})
