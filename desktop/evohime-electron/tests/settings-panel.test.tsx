// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { SettingsPanel } from '../src/renderer/src/SettingsPanel'

const calls: string[] = []

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
    invoke: (async (command: RendererCommand) => {
      calls.push(command)
      if (command === 'provider.get') {
        return ok({ provider: 'literouter', model: '', baseUrl: '', configured: false })
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

describe('settings panel', () => {
  it('requests provider references and renders config without secrets', async () => {
    const view = render(<SettingsPanel connection="connected" events={[]} />)
    expect(calls).toEqual(['provider.get', 'core.getModelConfig', 'core.listModelCatalog'])
    view.rerender(<SettingsPanel connection="connected" events={[
      event('model.config', { provider: 'openai-compatible', route: 'local', model: 'gpt', configured: true }),
      event('model.catalog', { mode: 'free', models: ['model:free'] })
    ]} />)
    expect(await screen.findByText('openai-compatible')).toBeTruthy()
    expect(screen.getByText('model:free')).toBeTruthy()
    expect(screen.queryByText('sk-test-secret')).toBeNull()
  })

  it('reloads the catalog when switching paid/free references', async () => {
    render(<SettingsPanel connection="connected" events={[]} />)
    await userEvent.click(screen.getByRole('button', { name: 'Paid' }))
    expect(calls.at(-1)).toBe('core.listModelCatalog')
  })
})
