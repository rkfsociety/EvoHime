/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import type { CoreEvent, TaskCheckpointProjection } from '../src/shared/api'
import { TaskCheckpointPanel } from '../src/renderer/src/TaskCheckpointPanel'

const projection: TaskCheckpointProjection = {
  schemaVersion: 1,
  checkpointId: 'task-checkpoint-1',
  taskId: 'task-1',
  workspaceId: 'workspace-1',
  parentCheckpointId: '',
  status: 'blocked',
  sourceEventSeq: 7,
  createdAt: 100,
  completedCount: 1,
  remainingCount: 1,
  blockerCount: 1,
  blockers: ['неизвестный внешний outcome'],
  refs: [{ kind: 'policy_snapshot', id: 'task-checkpoint-policy-v1', contentHash: 'hash-1', sensitivity: 'public' }],
  recoveryDisposition: 'blocked',
  recoveryWarning: 'Blind retry запрещён.',
  replayedEventTypes: ['run.recovery.blocked'],
  canRequestResume: false,
  replayedEventCount: 1,
  policyId: 'task-checkpoint-policy-v1',
  errorCode: ''
}

function installApi(): ReturnType<typeof vi.fn> {
  const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true }
  }
  return invoke
}

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
})

describe('TaskCheckpointPanel', () => {
  it('renders only the bounded Core projection', () => {
    installApi()
    const event: CoreEvent = {
      sequenceId: 0,
      taskId: 'task-1',
      eventType: 'task.checkpoint',
      payload: '',
      executionEvent: null,
      taskCheckpoint: projection
    }
    render(<TaskCheckpointPanel connection="connected" events={[event]} taskId="task-1" workspace="C:\\workspace" />)

    expect(screen.getByRole('heading', { name: 'Заблокирован' })).toBeTruthy()
    expect(screen.getByText('Blind retry запрещён.')).toBeTruthy()
    expect(screen.getByText('неизвестный внешний outcome')).toBeTruthy()
    expect(screen.getByText('task-checkpoint-policy-v1')).toBeTruthy()
    expect(screen.queryByText(/prompt|secret|reasoning/i)).toBeNull()
  })

  it('sends explicit recovery actions through the typed bridge', async () => {
    const invoke = installApi()
    const event: CoreEvent = {
      sequenceId: 0,
      taskId: 'task-1',
      eventType: 'task.checkpoint',
      payload: '',
      executionEvent: null,
      taskCheckpoint: { ...projection, recoveryDisposition: 'replayable', recoveryWarning: '', canRequestResume: true }
    }
    render(<TaskCheckpointPanel connection="connected" events={[event]} taskId="task-1" workspace="C:\\workspace" />)

    fireEvent.click(screen.getByRole('button', { name: 'Запросить reconciliation' }))

    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.resolveTaskCheckpoint'))).toBe(true)
    expect(invoke.mock.calls.find(([command]) => command === 'core.resolveTaskCheckpoint')?.[1]).toMatchObject({
      taskId: 'task-1',
      checkpointId: 'task-checkpoint-1',
      expectedSourceEventSeq: 7,
      action: 'request_resume'
    })
  })
})
