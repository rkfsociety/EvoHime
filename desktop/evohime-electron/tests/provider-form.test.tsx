// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { ProviderForm } from '../src/renderer/src/ProviderForm'
import { ProviderStateProvider } from '../src/renderer/src/provider-state'

/**
 * The credentials surface is the one place a user types a secret. These tests
 * pin that it never renders the stored value back and that a rejected write is
 * reported instead of being swallowed.
 */

const calls: { command: string; payload: unknown }[] = []
let saveOutcome: CommandOutcome<'provider.save'>
let selectOutcome: CommandOutcome<'provider.select'>

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

beforeEach(() => {
  calls.length = 0
  saveOutcome = ok({
    summary: {
      provider: 'literouter',
      model: 'deepseek:free',
      baseUrl: '',
      tier: 'free',
      configured: true
    },
    restarted: true
  })
  selectOutcome = ok({
    summary: {
      provider: 'ollama',
      model: '',
      baseUrl: 'http://127.0.0.1:11434/v1',
      tier: 'free',
      configured: true
    },
    restarted: true
  })
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'provider.get') {
        return ok({
          provider: 'literouter',
          model: 'deepseek:free',
          baseUrl: '',
          tier: 'free',
          configured: false,
          profiles: {
            literouter: { model: 'deepseek:free', baseUrl: '', tier: 'free', configured: false },
            openai_compatible: { model: '', baseUrl: '', tier: 'free', configured: false },
            openai_responses: { model: '', baseUrl: '', tier: 'free', configured: false },
            ollama: { model: '', baseUrl: 'http://127.0.0.1:11434/v1', tier: 'free', configured: true }
          }
        })
      }
      if (command === 'provider.select') return selectOutcome
      return saveOutcome
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

function renderProviderForm(): void {
  render(
    <ProviderStateProvider>
      <ProviderForm />
    </ProviderStateProvider>
  )
}

describe('provider form', () => {
  it('sends the key once and clears the field afterwards', async () => {
    renderProviderForm()
    expect(await screen.findByText('Ключ не задан')).toBeTruthy()

    const field = screen.getByLabelText('Ключ API') as HTMLInputElement
    // A secret must never be a readable input.
    expect(field.type).toBe('password')
    await userEvent.type(field, 'sk-secret-value')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    const save = calls.find((call) => call.command === 'provider.save')
    expect(save?.payload).toEqual({
      provider: 'literouter',
      apiKey: 'sk-secret-value',
      // The model belongs to the composer; settings only carry it through.
      model: 'deepseek:free',
      baseUrl: '',
      tier: 'free'
    })
    await waitFor(() => expect(screen.getByText(/Core перезапущен/)).toBeTruthy())
    // The stored value is never echoed back into the form.
    expect(field.value).toBe('')
    expect(screen.getByText('Ключ сохранён')).toBeTruthy()
  })

  it('keeps the provider settings block free of a separate coding-engine switch', async () => {
    renderProviderForm()
    expect(await screen.findByLabelText('Провайдер')).toBeTruthy()
    expect(screen.queryByLabelText('Движок coding-задач')).toBeNull()
  })

  it('automatically selects, persists and applies a configured provider', async () => {
    renderProviderForm()

    const provider = await screen.findByLabelText('Провайдер')
    await userEvent.selectOptions(provider, 'ollama')

    await waitFor(() => expect(calls).toContainEqual({ command: 'provider.select', payload: { provider: 'ollama' } }))
    expect(calls.some((call) => call.command === 'provider.save')).toBe(false)
    expect(await screen.findByText(/Провайдер выбран и сохранён/)).toBeTruthy()
    expect(screen.getByText('Локальный провайдер')).toBeTruthy()
  })

  it('surfaces a rejected write instead of reporting success', async () => {
    saveOutcome = { ok: false, code: 'invalid-payload', message: 'Адрес должен быть https.' }
    renderProviderForm()

    await userEvent.type(await screen.findByLabelText('Ключ API'), 'sk-value')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(screen.getByText('Адрес должен быть https.')).toBeTruthy()
    expect(screen.getByText('Ключ не задан')).toBeTruthy()
  })

  it('warns when the key was stored but Core did not come back', async () => {
    saveOutcome = ok({
      summary: { provider: 'literouter', model: '', baseUrl: '', tier: 'free', configured: true },
      restarted: false
    })
    renderProviderForm()

    await userEvent.type(await screen.findByLabelText('Ключ API'), 'sk-value')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    expect(await screen.findByText(/Core не перезапустился/)).toBeTruthy()
  })
})
