/** @vitest-environment jsdom */

import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'

import { latestRecoveryNotice } from '../src/renderer/src/recovery-state'
import { RecoveryBanner } from '../src/renderer/src/RecoveryBanner'

const event = (eventType: string, payload: Record<string, unknown>) => ({
  sequenceId: 1,
  taskId: 'task-1',
  eventType,
  payload: JSON.stringify(payload)
})

describe('recovery contract', () => {
  it('maps Core recovery events to safe UI states', () => {
    expect(latestRecoveryNotice([event('storage.progress', { phase: 'restore', message: 'reopen' })])).toMatchObject({
      state: 'RECOVERING',
      phase: 'restore',
      canCancel: false
    })
    expect(latestRecoveryNotice([event('approval.required', { approval_id: 'approval-1' })])?.state).toBe('WAITING_APPROVAL')
    expect(latestRecoveryNotice([event('run.recovery.blocked', { reason: 'reconcile' })])?.state).toBe('BLOCKED')
    expect(latestRecoveryNotice([event('task.failed', { error: 'safe error', request_id: 'request-1' })])?.state).toBe('FAILED')
  })

  it('shows only actions supported by the state', () => {
    render(<RecoveryBanner connection="connected" events={[event('task.failed', { error: 'safe error', request_id: 'request-1' })]} onOpenTask={vi.fn()} />)
    expect(screen.getByText('FAILED')).toBeTruthy()
    expect(screen.getByText('Перезапросить состояние')).toBeTruthy()
    expect(screen.getByText('Открыть детали')).toBeTruthy()
    expect(screen.queryByText('Открыть подтверждение')).toBeNull()
  })
})
