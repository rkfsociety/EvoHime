/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

import type { CoreEvent, GoalListProjection, GoalProjection } from '../src/shared/api'
import { PinnedGoalBar } from '../src/renderer/src/PinnedGoalBar'
import { parseGoalSlashCommand } from '../src/renderer/src/TaskTimeline'

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
  it('renders nothing above the composer when no goal exists', async () => {
    const invoke = installApi()
    const onRunGoal = vi.fn().mockResolvedValue(undefined)

    const { container } = render(
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

    expect(container.firstChild).toBeNull()
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('core.listGoals', { workspacePath: 'D:/repo', chatId: 'chat-1', limit: 32 })
    })
  })

  it('refreshes goals when the model executes goal_create', async () => {
    const invoke = installApi()
    const onRunGoal = vi.fn().mockResolvedValue(undefined)
    const toolOutputEvent = {
      sequenceId: 7,
      taskId: 'task-1',
      eventType: 'tool.output',
      payload: JSON.stringify({ tool_name: 'goal_create', output: 'created' }),
      executionEvent: null
    } as unknown as CoreEvent

    render(
      <PinnedGoalBar
        connection="connected"
        events={[toolOutputEvent]}
        workspace="D:/repo"
        chatId="chat-1"
        taskRunning={true}
        disabled={true}
        onRunGoal={onRunGoal}
      />
    )

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('core.listGoals', { workspacePath: 'D:/repo', chatId: 'chat-1', limit: 32 })
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

describe('parseGoalSlashCommand', () => {
  it('parses /goal command with objective and rejects empty /goal', () => {
    expect(parseGoalSlashCommand('/goal починить все тесты')).toEqual({
      isGoalCommand: true,
      objective: 'починить все тесты'
    })
    expect(parseGoalSlashCommand('  /GOAL   довести до конца  ')).toEqual({
      isGoalCommand: true,
      objective: 'довести до конца'
    })
    expect(parseGoalSlashCommand('/goal   ')).toEqual({
      isGoalCommand: true,
      objective: ''
    })
    expect(parseGoalSlashCommand('обычная задача')).toEqual({
      isGoalCommand: false
    })
  })
})
