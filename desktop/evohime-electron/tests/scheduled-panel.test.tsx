// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { ScheduledPanel } from '../src/renderer/src/ScheduledPanel'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 1, taskId: '', eventType, payload: JSON.stringify(payload) }
}

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('scheduled panel', () => {
  it('requests and renders Core-owned schedules', async () => {
    render(
      <ScheduledPanel
        connection="connected"
        workspace={'C:\\work\\repo'}
        events={[event('automation.schedules', {
          schedules: [{ schedule_id: 'morning-review', definition_id: 'review', revision: 2, hour: 9, minute: 5, timezone_minutes: 120, enabled: true, last_slot: null }],
          error_code: ''
        })]}
      />
    )

    expect(await screen.findByRole('heading', { name: 'review' })).toBeTruthy()
    expect(screen.getByText('09:05 · UTC+02:00 · ревизия 2')).toBeTruthy()
    await waitFor(() => expect(calls).toContainEqual({
      command: 'automation.listSchedules',
      payload: { ownerScope: 'C:\\work\\repo', limit: 64 }
    }))

    await userEvent.click(screen.getByRole('button', { name: 'Пауза' }))
    await waitFor(() => expect(calls).toContainEqual({
      command: 'automation.setScheduleEnabled',
      payload: { scheduleId: 'morning-review', enabled: false }
    }))
  })
})
