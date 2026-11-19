// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { AnalysisKernelPanel } from '../src/renderer/src/AnalysisKernelPanel'
import type { CoreEvent } from '../src/shared/api'

function makeEvent(overrides: Partial<CoreEvent> = {}): CoreEvent {
  return {
    sequenceId: 1,
    taskId: 'task-1',
    eventType: 'task.started',
    payload: '{}',
    executionEvent: null,
    coreInstanceId: 'core-1',
    sessionEpoch: 1,
    ...overrides
  }
}

describe('AnalysisKernelPanel', () => {
  const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })

  beforeEach(() => {
    invoke.mockClear()
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { invoke } }
  })

  afterEach(() => {
    cleanup()
    delete (window as unknown as { evohime?: unknown }).evohime
  })

  it('requests kernels on mount and shows honest empty state when none exist', async () => {
    render(<AnalysisKernelPanel connection="connected" events={[makeEvent()]} taskId="task-1" />)

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('core.listAnalysisKernels', {
        taskId: 'task-1',
        maxItems: 32
      })
    })
    expect(
      screen.getByText(/Для задачи пока нет аналитических ядер/)
    ).toBeTruthy()
  })

  it('merges list response with live events, deduplicates by kernelId, and hides values', async () => {
    const events: CoreEvent[] = [
      makeEvent({
        sequenceId: 1,
        eventType: 'analysis_kernel.list',
        payload: JSON.stringify({
          taskId: 'task-1',
          errorCode: '',
          kernels: [
            {
              schemaVersion: 1,
              kernelId: 'kernel-b',
              taskId: 'task-1',
              workspaceId: 'ws-1',
              runtimeVersion: 'trusted-local-1',
              packageManifestHash: 'a'.repeat(64),
              policyHash: 'b'.repeat(64),
              status: 'running',
              revision: 1,
              limitsJson: '{}',
              objectCount: 2,
              truncated: false,
              errorCode: ''
            },
            {
              schemaVersion: 1,
              kernelId: 'kernel-a',
              taskId: 'task-1',
              workspaceId: 'ws-1',
              runtimeVersion: 'trusted-local-1',
              packageManifestHash: 'a'.repeat(64),
              policyHash: 'b'.repeat(64),
              status: 'created',
              revision: 0,
              limitsJson: '{}',
              objectCount: 0,
              truncated: false,
              errorCode: ''
            }
          ]
        })
      }),
      makeEvent({
        sequenceId: 2,
        eventType: 'analysis_kernel.updated',
        analysisKernel: {
          schemaVersion: 1,
          kernelId: 'kernel-a',
          taskId: 'task-1',
          workspaceId: 'ws-1',
          runtimeVersion: 'trusted-local-1',
          packageManifestHash: 'a'.repeat(64),
          policyHash: 'b'.repeat(64),
          status: 'stopped',
          revision: 2,
          limitsJson: '{}',
          objectCount: 1,
          truncated: false,
          errorCode: ''
        }
      })
    ]

    const { container } = render(
      <AnalysisKernelPanel connection="connected" events={events} taskId="task-1" />
    )

    const items = container.querySelectorAll('.stack-list__item')
    expect(items.length).toBe(2)
    expect(items[0]?.textContent).toContain('kernel-a')
    expect(items[0]?.textContent).toContain('revision: 2')
    expect(items[1]?.textContent).toContain('kernel-b')

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(1)
    })
    const refreshButton = screen.getByRole('button', { name: 'Обновить' }) as HTMLButtonElement
    await waitFor(() => {
      expect(refreshButton.disabled).toBe(false)
    })
    fireEvent.click(refreshButton)
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(2)
    })
  })
})
