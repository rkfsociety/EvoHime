// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { ChatProviderMode, CommandOutcome, EvoHimeApiV1, ProviderSummary, RendererCommand } from '../src/shared/api'
import { ChatProviderPicker } from '../src/renderer/src/ChatProviderPicker'
import { ProviderForm } from '../src/renderer/src/ProviderForm'
import { ProviderStateProvider } from '../src/renderer/src/provider-state'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function summary(literouterConfigured: boolean): ProviderSummary {
  return {
    provider: 'literouter',
    model: 'deepseek-r1-0528:free',
    baseUrl: '',
    tier: 'free',
    configured: literouterConfigured,
    profiles: {
      literouter: { model: 'deepseek-r1-0528:free', baseUrl: '', tier: 'free', configured: literouterConfigured },
      openai_compatible: { model: '', baseUrl: '', tier: 'free', configured: false },
      openai_responses: { model: '', baseUrl: '', tier: 'free', configured: false },
      ollama: { model: 'llama3.2', baseUrl: 'http://127.0.0.1:11434/v1', tier: 'free', configured: true }
    }
  }
}

beforeEach(() => {
  calls.length = 0
  let current = summary(false)
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'provider.get') return ok(current)
      if (command === 'provider.save') {
        current = summary(true)
        return ok({ summary: current, restarted: true })
      }
      if (command === 'codex.getStatus') {
        return ok({ installed: false, installing: false, loggingIn: false, available: false, loggedIn: false, selectedModel: '', models: [], rateLimits: [], lastUpdatedMs: 1, error: null })
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

describe('shared provider state', () => {
  it('updates the chat provider list immediately after settings save', async () => {
    const selected: string[] = []
    function ChatPickerHarness(): React.JSX.Element {
      const [provider, setProvider] = useState<ChatProviderMode>('ollama')
      return <ChatProviderPicker connection="connected" value={provider} onChange={(next) => { selected.push(next); setProvider(next) }} />
    }

    render(
      <ProviderStateProvider>
        <ProviderForm />
        <ChatPickerHarness />
      </ProviderStateProvider>
    )

    const key = await screen.findByLabelText('Ключ API')
    await userEvent.type(key, 'lr-test-key')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    const picker = screen.getByRole('combobox', { name: 'Провайдер задачи' })
    await waitFor(() => {
      expect(picker.querySelector('option[value="literouter"]')).toBeTruthy()
    })
    expect(picker.querySelector('option[value="ollama"]')).toBeTruthy()
    expect((picker as HTMLSelectElement).value).toBe('literouter')
    expect(selected).toContain('literouter')
    expect(calls.filter((call) => call.command === 'provider.get')).toHaveLength(1)
  })
})
