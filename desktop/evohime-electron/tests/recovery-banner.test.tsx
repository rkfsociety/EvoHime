/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import { latestRecoveryNotice } from '../src/renderer/src/recovery-state'
import { RecoveryBanner } from '../src/renderer/src/RecoveryBanner'

const event = (eventType: string, payload: Record<string, unknown>, sequenceId = 1) => ({
  sequenceId,
  taskId: 'task-1',
  eventType,
  payload: JSON.stringify(payload)
})

/** Мост preload с управляемым ответом на shell.requestResync. */
function installApi(outcome: unknown): ReturnType<typeof vi.fn> {
  const invoke = vi.fn().mockResolvedValue(outcome)
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true }
  }
  return invoke
}

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
})

describe('recovery contract', () => {
  it('maps Core recovery events to safe UI states', () => {
    expect(latestRecoveryNotice([event('storage.progress', { phase: 'restore', message: 'reopen' })])).toMatchObject({
      state: 'RECOVERING',
      phase: 'restore',
      reasonCode: 'storage.progress',
      canCancel: false
    })
    expect(latestRecoveryNotice([event('approval.required', { approval_id: 'approval-1' })])?.state).toBe('WAITING_APPROVAL')
    expect(latestRecoveryNotice([event('run.recovery.blocked', { reason: 'reconcile' })])?.state).toBe('BLOCKED')
    expect(latestRecoveryNotice([event('run.reconciliation.completed', { run_id: 'run-1' })])?.state).toBe('RESUMABLE')
    expect(latestRecoveryNotice([event('task.failed', { error: 'safe error', request_id: 'request-1' })])?.state).toBe('FAILED')
    expect(latestRecoveryNotice([event('run.unknown_outcome', { reason_code: 'dispatch_marker_present', reason: 'unknown' })])?.state).toBe('UNKNOWN_OUTCOME')
    expect(latestRecoveryNotice([event('storage.progress', { operation_id: 'op-1', can_cancel: true })])?.canCancel).toBe(true)
  })

  it('takes the newest event, so a stale failure does not outlive recovery', () => {
    // App хранит события новыми вперёд.
    const events = [
      event('storage.progress', { phase: 'restore', message: 'reopen' }, 2),
      event('task.failed', { error: 'safe error', request_id: 'request-1' }, 1)
    ]
    expect(latestRecoveryNotice(events)?.state).toBe('RECOVERING')
  })

  it('does not resurrect approval after the task was stopped', () => {
    const events = [
      event('task.stopped', {}, 3),
      event('approval.required', { approval_id: 'approval-1' }, 2)
    ]
    expect(latestRecoveryNotice(events)).toBeNull()
  })

  it('reads externally tagged Core approval payloads', () => {
    const tagged = event('approval.required', {}, 4)
    tagged.payload = JSON.stringify({
      ApprovalRequired: {
        approval_id: 'approval-tagged',
        can_cancel: true
      }
    })
    expect(latestRecoveryNotice([tagged])).toMatchObject({
      state: 'WAITING_APPROVAL',
      correlationId: 'approval-tagged',
      canCancel: true
    })
  })

  it('shows only actions supported by the state', () => {
    render(<RecoveryBanner connection="connected" events={[event('task.failed', { error: 'safe error', request_id: 'request-1' })]} onOpenTask={vi.fn()} />)
    expect(screen.getByText('FAILED')).toBeTruthy()
    expect(screen.getByText('Перезапросить состояние')).toBeTruthy()
    expect(screen.getByText('Открыть детали')).toBeTruthy()
    expect(screen.queryByText('Открыть подтверждение')).toBeNull()
  })

  it('does not surface task history in the shell-wide banner', () => {
    const { container } = render(
      <RecoveryBanner
        connection="connected"
        events={[event('task.failed', { error: 'old task failure', request_id: 'request-1' })]}
        onOpenTask={vi.fn()}
        taskScoped={false}
      />
    )
    expect(container.querySelector('.recovery-banner')).toBeNull()
  })

  it('opens the redacted details of the failed event', () => {
    const onOpenTask = vi.fn()
    render(<RecoveryBanner connection="connected" events={[event('task.failed', { error: 'safe error', request_id: 'request-1' })]} onOpenTask={onOpenTask} />)

    fireEvent.click(screen.getByText('Открыть детали'))

    expect(onOpenTask).toHaveBeenCalled()
    expect(screen.getByText(/task\.failed · seq/)).toBeTruthy()
    expect(screen.getByText('request_id')).toBeTruthy()
    fireEvent.click(screen.getByText('Скрыть детали'))
    expect(screen.queryByText('request_id')).toBeNull()
  })

  it('reports the outcome of a resync request', async () => {
    const invoke = installApi({ ok: true, value: { accepted: true } })
    render(<RecoveryBanner connection="connected" events={[event('task.failed', { error: 'safe error', request_id: 'request-1' })]} onOpenTask={vi.fn()} />)

    fireEvent.click(screen.getByText('Перезапросить состояние'))

    expect(invoke).toHaveBeenCalledWith('shell.requestResync', {})
    expect(await screen.findByText('Состояние запрошено — жду события от Core.')).toBeTruthy()
  })

  it('explains why a resync is impossible instead of staying silent', async () => {
    const invoke = installApi({ ok: true, value: { accepted: true } })
    render(<RecoveryBanner connection="fatal" events={[event('task.failed', { error: 'safe error', request_id: 'request-1' })]} onOpenTask={vi.fn()} />)

    fireEvent.click(screen.getByText('Перезапросить состояние'))

    expect(invoke).not.toHaveBeenCalled()
    expect(await screen.findByText('Core не подключён: запрос состояния невозможен.')).toBeTruthy()
  })

  it('can be dismissed, because the failed event stays in the stream', () => {
    const { container } = render(
      <RecoveryBanner connection="connected" events={[event('task.failed', { error: 'safe error', request_id: 'request-1' })]} onOpenTask={vi.fn()} />
    )

    fireEvent.click(screen.getByLabelText('Скрыть уведомление'))

    expect(container.querySelector('.recovery-banner')).toBeNull()
  })
})
