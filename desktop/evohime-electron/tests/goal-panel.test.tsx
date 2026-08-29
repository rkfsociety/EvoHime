/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import type { CoreEvent, GoalListProjection, GoalProjection } from '../src/shared/api'
import { GoalPanel } from '../src/renderer/src/GoalPanel'

const goal: GoalProjection = {
  schemaVersion: 1,
  goalId: 'goal-1',
  version: 2,
  workspaceId: 'workspace-1',
  chatId: '',
  objective: 'Подготовить релиз',
  successCriteria: [{
    id: 'criterion-1',
    kind: 'manual',
    statement: 'Проверки проходят',
    status: 'pending',
    evidenceRef: 'test-run-1',
    verifierId: 'core:tests',
    verifierVersion: '',
    verifiedAtMs: 0,
    provenance: 'user'
  }],
  status: 'active',
  progressSummary: '0 из 1 критериев подтверждено',
  completedCriteria: [],
  remainingCriteria: ['criterion-1'],
  blockers: [],
  nextAction: 'Подтвердить результат',
  workflowRunIds: ['workflow-1'],
  childRunIds: ['child-1'],
  checkpointId: 'checkpoint-1',
  tokenBudget: 1000,
  costBudgetMicros: 0,
  continuationBudget: 1,
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
  }
}

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
})

describe('GoalPanel', () => {
  it('renders only the bounded Core projection and requests the list', async () => {
    const invoke = installApi()
    render(<GoalPanel connection="connected" events={[listEvent({ schemaVersion: 1, goals: [goal], errorCode: '', truncated: false })]} workspace={'C:\\workspace'} />)

    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.listGoals'))).toBe(true)
    expect(screen.getByText('Подготовить релиз')).toBeTruthy()
    expect(screen.getByText('Проверки проходят')).toBeTruthy()
    expect(screen.getByText('Бюджет: tokens 1000 · cost — · продолжений 1')).toBeTruthy()
    expect(screen.getByText('Workflow: workflow-1')).toBeTruthy()
    expect(screen.getByText('Дочерние runs: child-1')).toBeTruthy()
    expect(screen.getByText('Checkpoint: checkpoint-1')).toBeTruthy()
    expect(screen.getByText('Evidence criterion-1: test-run-1')).toBeTruthy()
    expect(screen.queryByText(/prompt|secret|reasoning/i)).toBeNull()
  })

  it('sends creation, transition, and criterion verification through typed commands', async () => {
    const invoke = installApi()
    render(<GoalPanel connection="connected" events={[listEvent({ schemaVersion: 1, goals: [goal], errorCode: '', truncated: false })]} workspace={'C:\\workspace'} />)

    fireEvent.change(screen.getByRole('textbox', { name: 'Новая цель' }), { target: { value: 'Новая поставка' } })
    fireEvent.change(screen.getByRole('textbox', { name: 'Критерий успеха' }), { target: { value: 'Артефакт опубликован' } })
    fireEvent.click(screen.getByRole('button', { name: 'Создать цель' }))

    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.createGoal'))).toBe(true)
    expect(invoke.mock.calls.find(([command]) => command === 'core.createGoal')?.[1]).toMatchObject({
      workspacePath: 'C:\\workspace',
      objective: 'Новая поставка',
      successCriteria: [{ id: 'criterion-1', kind: 'manual', statement: 'Артефакт опубликован' }]
    })

    await vi.waitFor(() => expect(screen.getByRole('button', { name: 'Пауза' }).hasAttribute('disabled')).toBe(false))
    fireEvent.click(screen.getByRole('button', { name: 'Пауза' }))

    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.pauseGoal'))).toBe(true)
    expect(invoke.mock.calls.find(([command]) => command === 'core.pauseGoal')?.[1]).toMatchObject({
      goalId: 'goal-1', expectedVersion: 2
    })

    await vi.waitFor(() => expect(screen.getByRole('button', { name: 'Подтвердить' }).hasAttribute('disabled')).toBe(false))
    fireEvent.click(screen.getByRole('button', { name: 'Подтвердить' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.verifyGoalCriterion'))).toBe(true)
    expect(invoke.mock.calls.find(([command]) => command === 'core.verifyGoalCriterion')?.[1]).toMatchObject({
      goalId: 'goal-1', expectedVersion: 2, criterionId: 'criterion-1'
    })
    expect(invoke.mock.calls.find(([command]) => command === 'core.verifyGoalCriterion')?.[1]).not.toHaveProperty('evidenceRef')
  })
})
