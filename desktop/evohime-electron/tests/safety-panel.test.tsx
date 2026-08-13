// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { SafetyPanel } from '../src/renderer/src/SafetyPanel'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
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

describe('safety panel', () => {
  it('changes permission mode and starts Core Doctor', async () => {
    render(<SafetyPanel connection="connected" events={[]} />)
    await userEvent.click(screen.getByRole('button', { name: 'Только чтение' }))
    await userEvent.click(screen.getByRole('button', { name: 'Запустить Doctor' }))
    expect(calls.map((call) => call.command)).toEqual(['core.setPermissionMode', 'core.runDoctor'])
  })

  it('shows redacted Doctor output and storage progress', async () => {
    render(<SafetyPanel connection="connected" events={[
      event('doctor.report', { checks: [{ id: 'storage', status: 'ok', summary: 'SQLite доступна' }] }),
      event('storage.progress', { phase: 'backup', completed: 3, total: 5, message: 'writing backup' })
    ]} />)
    expect(await screen.findByText(/storage: ok/)).toBeTruthy()
    expect(screen.getByText(/backup · 3 \/ 5/)).toBeTruthy()
  })
})
