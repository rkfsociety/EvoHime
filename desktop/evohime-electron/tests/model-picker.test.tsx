// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type {
  CommandOutcome,
  CoreEvent,
  EvoHimeApiV1,
  ModelTier,
  RendererCommand
} from '../src/shared/api'
import { ModelPicker } from '../src/renderer/src/ModelPicker'

/**
 * The composer's model selection is driven by the provider's own catalogue.
 * These tests pin that the shell asks for the tier the user configured, offers
 * exactly what came back, and reports a catalogue failure where the user is.
 */

const calls: { command: string; payload: unknown }[] = []
let tier: ModelTier = 'free'

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

beforeEach(() => {
  calls.length = 0
  tier = 'free'
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'provider.get') {
        return ok({ provider: 'literouter', model: '', baseUrl: '', tier, configured: true })
      }
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('model picker', () => {
  it('requests the configured tier and offers what the provider returned', async () => {
    tier = 'paid'
    const view = render(<ModelPicker connection="connected" events={[]} />)

    await waitFor(() =>
      expect(calls).toContainEqual({ command: 'core.listModelCatalog', payload: { mode: 'paid' } })
    )

    view.rerender(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', { mode: 'paid', models: ['gpt-4o-mini', 'claude'] })]}
      />
    )

    const select = (await screen.findByLabelText('Модель')) as HTMLSelectElement
    expect([...select.options].map((option) => option.value)).toEqual(['gpt-4o-mini', 'claude'])
  })

  it('tells Core about a selection without restarting anything', async () => {
    render(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', { mode: 'free', models: ['a:free', 'b:free'] })]}
      />
    )

    await userEvent.selectOptions(await screen.findByLabelText('Модель'), 'b:free')

    expect(calls).toContainEqual({ command: 'core.selectModel', payload: { model: 'b:free' } })
  })

  it('points at the key when the catalogue could not be read', async () => {
    render(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', { mode: 'free', models: [], error: 'provider API key is not configured' })]}
      />
    )

    expect(await screen.findByText(/проверь ключ в настройках/i)).toBeTruthy()
    expect(screen.queryByLabelText('Модель')).toBeNull()
  })

  it('stays out of the composer while Core is unreachable', () => {
    const { container } = render(<ModelPicker connection="reconnecting" events={[]} />)
    expect(container.firstChild).toBeNull()
  })
})
