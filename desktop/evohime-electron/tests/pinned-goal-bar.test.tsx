/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

import type { CoreEvent, GoalListProjection, GoalProjection } from '../src/shared/api'
import { PinnedGoalBar } from '../src/renderer/src/PinnedGoalBar'

const activeGoal: GoalProjection = {
  schemaVersion: 1,
  goalId: 'goal-1',
  version: 2,
  workspaceId: 'workspace-1',
  chatId: 'chat-1',
  objective: 'Починить все тесты модуля',
  successCriteria: [{
    id: 'criterion-1',
    kind: 'manual',
    statement: 'Тесты зелёные',
    status: 'pending',
    evidenceRef: '',
    verifierId: '',
    verifierVersion: '',
    verifiedAtMs: 0,
    provenance: 'user'
  }],
  status: 'active',
  progressSummary: '0 из 1 критериев подтверждено',
  completedCriteria: [],
  remainingCriteria: ['criterion-1'],
  blockers: [],
  nextAction: 'Запустить тесты',
  workflowRunIds: [],
  childRunIds: [],
  checkpointId: '',
  tokenBudget: 0,
  costBudgetMicros: 0,
  continuationBudget: 12,
  createdAtMs: 100,
  updatedAtMs: 200,
  contentHash: 'hash-goal-1',
  recoveryWarning: '',
  errorCode: ''
}

function installApi() {
  const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true }
  }
  return invoke
}

function listEvent(list: GoalListProjection): CoreEvent {
  return {
    sequenceId: 1,
    taskId: '',
    eventType: 'goal.list',
    payload: '',
    executionEvent: null,
    goalList: list
  } as unknown as CoreEvent
}

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
})

describe('PinnedGoalBar', () => {
  it('creates a goal and starts execution immediately from the bar form', async () => {
    const invoke = installApi()
    const onRunGoal = vi.fn().mockResolvedValue(undefined)

    render(
      <PinnedGoalBar
        connection="connected"
        events={[]}
        workspace="D:/repo"
        chatId="chat-1"
        taskRunning={false}
        disabled={false}
        onRunGoal={onRunGoal}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /Поставить цель/i }))
    fireEvent.change(screen.getByPlaceholderText(/Сформулируй цель/i), {
      target: { value: 'Довести фичу до конца' }
    })
    fireEvent.click(screen.getByRole('button', { name: /Поставить и выполнить/i }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'core.createGoal',
        expect.objectContaining({
          workspacePath: 'D:/repo',
          chatId: 'chat-1',
          objective: 'Довести фичу до конца',
          continuationBudget: 12
        })
      )
      expect(onRunGoal).toHaveBeenCalledWith(
        expect.stringMatching(/^goal-/),
        'Довести фичу до конца'
      )
    })
  })

  it('displays the pinned active goal and pauses it on user action', async () => {
    const invoke = installApi()
    const onRunGoal = vi.fn().mockResolvedValue(undefined)
    const events = [listEvent({ schemaVersion: 1, workspaceId: 'workspace-1', goals: [activeGoal], errorCode: '' })]

    render(
      <PinnedGoalBar
        connection="connected"
        events={events}
        workspace="D:/repo"
        chatId="chat-1"
        taskRunning={false}
        disabled={false}
        onRunGoal={onRunGoal}
      />
    )

    expect(screen.getByText('Починить все тесты модуля')).toBeTruthy()
    expect(screen.getByText('0/1')).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Пауза' }))
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'core.pauseGoal',
        expect.objectContaining({
          goalId: 'goal-1',
          expectedVersion: 2
        })
      )
    })
  })
})
