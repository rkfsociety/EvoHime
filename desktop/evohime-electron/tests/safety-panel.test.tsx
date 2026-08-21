// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
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
    openExternal: async () => true,
    pathForFile: () => ''
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('панель безопасности', () => {
  it('называет микрофон отдельным разрешением, не подчинённым общему режиму', () => {
    render(<SafetyPanel connection="connected" events={[]} />)
    expect(screen.getByText('Постоянное слушание микрофона')).toBeTruthy()
    expect(screen.getByText(/Смена общего режима доступа его не трогает/i)).toBeTruthy()
    expect(screen.getAllByText('следует общему режиму доступа над полем ввода').length).toBe(8)
  })

  it('включение микрофона идёт той же командой, что и панель «Слух»', async () => {
    render(<SafetyPanel connection="connected" events={[]} />)
    await userEvent.click(screen.getByRole('button', { name: 'Включить' }))
    expect(calls.find((call) => call.command === 'ambient.setListening')?.payload).toMatchObject({
      enabled: true,
      paused: false
    })
  })

  it('при активном слушании предлагает выключение, а не включение', async () => {
    render(
      <SafetyPanel
        connection="connected"
        events={[event('ambient.state', { state: 'listening', reason: 'user_request' })]}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: 'Выключить' }))
    expect(calls.find((call) => call.command === 'ambient.setListening')?.payload).toMatchObject({
      enabled: false
    })
  })

  it('считает высказывания за последний час и честно называет неподключённый источник', async () => {
    const now = Date.now()
    render(
      <SafetyPanel
        connection="connected"
        events={[
          event('ambient.episodes', {
            episodes: [
              {
                episode_id: 'ep-1',
                started_at_ms: now - 10 * 60 * 1000,
                speech_duration_ms: 5_000,
                utterance_count: 3,
                extraction_state: 'disabled'
              },
              {
                episode_id: 'ep-0',
                started_at_ms: now - 5 * 60 * 60 * 1000,
                speech_duration_ms: 5_000,
                utterance_count: 40,
                extraction_state: 'disabled'
              }
            ],
            next_cursor: ''
          })
        ]}
      />
    )
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain('высказываний: 3')
    })
    expect(screen.getByRole('status').textContent).toContain('кандидатов памяти: не подключено')
  })
})
