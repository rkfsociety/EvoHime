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
      if (command === 'codex.getStatus') {
        return ok({
          installed: true,
          installing: false,
          loggingIn: false,
          available: true,
          loggedIn: true,
          selectedModel: 'gpt-5.5',
          models: [{
            id: 'gpt-5.5',
            model: 'gpt-5.5',
            displayName: 'GPT-5.5',
            description: '',
            defaultReasoningEffort: 'medium',
            supportedReasoningEfforts: ['medium'],
            isDefault: true
          }],
          rateLimits: [{
            limitId: 'codex',
            planType: 'plus',
            primary: { usedPercent: 20, remainingPercent: 80, resetsAt: 1_800_000_000, windowDurationMins: 300 },
            secondary: { usedPercent: 40, remainingPercent: 60, resetsAt: 1_800_600_000, windowDurationMins: 10080 },
            individualRemainingPercent: null,
            individualResetsAt: null,
            reached: false
          }],
          lastUpdatedMs: 1_700_000_000_000,
          error: null
        })
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
  it('filters a long catalogue instead of making the user scroll it', async () => {
    render(
      <ModelPicker
        connection="connected"
        events={[
          event('model.catalog', {
            mode: 'free',
            models: ['gemini-2.5-flash:free', 'gpt-5-nano:free', 'grok-4.1:free']
          })
        ]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: /Модель/ }))
    await userEvent.type(screen.getByLabelText('Поиск модели'), 'gpt')

    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'gpt-5-nano:free'
    ])
  })

  it('offers every model returned by the provider catalogue', async () => {
    render(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', {
          mode: 'free',
          models: ['mythomax-l2-13b:free', 'usable:free']
        })]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: /Модель/ }))
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual(['mythomax-l2-13b:free', 'usable:free'])
  })

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

    await userEvent.click(await screen.findByRole('button', { name: /Модель/ }))
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'gpt-4o-mini',
      'claude'
    ])
  })

  it('tells Core about a selection without restarting anything', async () => {
    render(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', { mode: 'free', models: ['a:free', 'b:free'] })]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: /Модель/ }))
    await userEvent.click(screen.getByRole('option', { name: 'b:free' }))

    expect(calls).toContainEqual({ command: 'core.selectModel', payload: { model: 'b:free' } })
  })

  it('commits to the model it displays instead of leaving Core on its default', async () => {
    // Regression: the dropdown rendered the first option while Core still used
    // the route default, so a task ran against a model the user never saw.
    render(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', { mode: 'free', models: ['first:free', 'second:free'] })]}
      />
    )

    await waitFor(() =>
      expect(calls).toContainEqual({
        command: 'core.selectModel',
        payload: { model: 'first:free' }
      })
    )
  })

  it('points at the key when the catalogue could not be read', async () => {
    render(
      <ModelPicker
        connection="connected"
        events={[event('model.catalog', { mode: 'free', models: [], error: 'provider API key is not configured' })]}
      />
    )

    expect(await screen.findByText(/проверь ключ в настройках/i)).toBeTruthy()
    expect(screen.queryByRole('button', { name: /Модель/ })).toBeNull()
  })

  it('stays out of the composer while Core is unreachable', () => {
    const { container } = render(<ModelPicker connection="reconnecting" events={[]} />)
    expect(container.firstChild).toBeNull()
  })

  it('shows Codex five-hour and weekly limits below its model', async () => {
    render(<ModelPicker connection="connected" events={[]} provider="codex_cli" />)

    expect(await screen.findByText('5 часов: осталось 80%')).toBeTruthy()
    expect(screen.getByText('Неделя: осталось 60%')).toBeTruthy()
    expect(screen.getByTestId('codex-composer-limits')).toBeTruthy()
  })
})
