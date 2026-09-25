// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
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
let providerGetSummary: unknown

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

beforeEach(() => {
  calls.length = 0
  providerGetSummary = {
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
  }
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
        return ok(providerGetSummary)
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

function renderProviderForm(events: readonly CoreEvent[] = []): void {
  render(
    <ProviderStateProvider>
      <ProviderForm events={events} />
    </ProviderStateProvider>
  )
}

describe('provider form', () => {
  it('explains that provider activation does not confirm no-cost access', async () => {
    providerGetSummary = {
      provider: 'openai_compatible',
      model: 'author/model:free',
      baseUrl: 'https://openrouter.ai/api/v1',
      tier: 'free',
      configured: true,
      profileId: 'openrouter'
    }
    renderProviderForm([{
      sequenceId: 3,
      taskId: '',
      eventType: 'free_access.probe',
      payload: JSON.stringify({
        state: 'activation_required',
        strict_eligible: false,
        failure_code: 'activation_required',
        redacted: true
      })
    }])

    expect(await screen.findByText(/требует активацию или смену плана/i)).toBeTruthy()
    expect(screen.getByText(/бесплатность запроса не подтверждена/i)).toBeTruthy()
  })

  it('shows the Core-owned catalog state without exposing credentials', async () => {
    renderProviderForm([{
      sequenceId: 1,
      taskId: '',
      eventType: 'model.catalog',
      payload: JSON.stringify({
        provider_catalog: {
          provider: { credential_status: 'configured' },
          catalog: { state: 'stale' }
        }
      })
    }])

    expect(await screen.findByText(/показан кэш каталога, маршрутизация остановлена/i)).toBeTruthy()
    expect(screen.queryByText(/api.?key|secret/i)).toBeNull()
  })

  it('explains a typed model-not-found catalog outcome', async () => {
    renderProviderForm([{
      sequenceId: 2,
      taskId: '',
      eventType: 'model.catalog',
      payload: JSON.stringify({
        provider_catalog: {
          provider: { credential_status: 'configured' },
          catalog: { state: 'unavailable', failure_code: 'model_not_found' }
        }
      })
    }])

    expect(await screen.findByText(/модель не найдена у провайдера/i)).toBeTruthy()
  })

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
      tier: 'free',
      freeAccessProbePolicy: 'disabled',
      acknowledgeProbePossibleCost: false,
      freeAccessRoutingMode: 'any',
      allowPaidFallback: false
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

  it('selects a vendor profile and requests a Cloudflare account ID separately from its token', async () => {
    selectOutcome = ok({
      summary: {
        provider: 'openai_compatible',
        model: '',
        baseUrl: 'https://api.openai.com/v1',
        tier: 'free',
        configured: false,
        profileId: 'openai',
        profiles: {
          openai_compatible: {
            model: '', baseUrl: 'https://api.openai.com/v1', tier: 'free', configured: false, profileId: 'openai'
          }
        }
      },
      restarted: true
    })
    renderProviderForm()

    await userEvent.selectOptions(await screen.findByLabelText('Провайдер'), 'openai_compatible')
    const profile = await screen.findByLabelText('Профиль провайдера')
    await userEvent.selectOptions(profile, 'cloudflare_workers_ai')
    await userEvent.type(await screen.findByLabelText('Cloudflare Account ID'), '0123456789abcdef0123456789abcdef')
    expect((screen.getByLabelText('Адрес API') as HTMLInputElement).disabled).toBe(true)
    await userEvent.type(screen.getByLabelText('Ключ API'), 'cf-token')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    await waitFor(() => expect(calls).toContainEqual({
      command: 'provider.save',
      payload: expect.objectContaining({
        provider: 'openai_compatible',
        profileId: 'cloudflare_workers_ai',
        accountId: '0123456789abcdef0123456789abcdef',
        baseUrl: ''
      })
    }))
  })

  it('requires a one-shot warning before requesting an OpenRouter free-access probe', async () => {
    selectOutcome = ok({
      summary: {
        provider: 'openai_compatible',
        model: 'author/model:free',
        baseUrl: 'https://openrouter.ai/api/v1',
        tier: 'free',
        configured: true,
        profileId: 'openrouter',
        profiles: {
          literouter: { model: '', baseUrl: '', tier: 'free', configured: false },
          openai_compatible: {
            model: 'author/model:free',
            baseUrl: 'https://openrouter.ai/api/v1',
            tier: 'free',
            configured: true,
            profileId: 'openrouter'
          },
          openai_responses: { model: '', baseUrl: '', tier: 'free', configured: false },
          ollama: { model: '', baseUrl: 'http://127.0.0.1:11434/v1', tier: 'free', configured: true }
        }
      },
      restarted: true
    })
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true)
    renderProviderForm()

    await userEvent.selectOptions(await screen.findByLabelText('Провайдер'), 'openai_compatible')
    const probe = await screen.findByRole('button', { name: 'Проверить текущую модель' })
    await userEvent.click(probe)

    expect(confirm).toHaveBeenCalledOnce()
    expect(calls).toContainEqual({
      command: 'provider.verifyFreeAccess',
      payload: { modelId: 'author/model:free', confirmPossibleCost: true }
    })
    expect(await screen.findByText(/Проверка выполняется/)).toBeTruthy()
    confirm.mockRestore()
  })

  it('requires persistent cost consent before saving an automatic OpenRouter probe policy', async () => {
    selectOutcome = ok({
      summary: {
        provider: 'openai_compatible',
        model: 'author/model:free',
        baseUrl: 'https://openrouter.ai/api/v1',
        tier: 'free',
        configured: true,
        profileId: 'openrouter',
        profiles: {
          literouter: { model: '', baseUrl: '', tier: 'free', configured: false },
          openai_compatible: {
            model: 'author/model:free', baseUrl: 'https://openrouter.ai/api/v1', tier: 'free',
            configured: true, profileId: 'openrouter', freeAccessProbePolicy: 'disabled'
          },
          openai_responses: { model: '', baseUrl: '', tier: 'free', configured: false },
          ollama: { model: '', baseUrl: 'http://127.0.0.1:11434/v1', tier: 'free', configured: true }
        },
        freeAccessRoutingMode: 'any',
        allowPaidFallback: false
      },
      restarted: true
    })
    renderProviderForm()
    await userEvent.selectOptions(await screen.findByLabelText('Провайдер'), 'openai_compatible')
    await userEvent.selectOptions(await screen.findByLabelText('Как собирать данные'), 'on_first_use')
    const save = screen.getByRole('button', { name: 'Сохранить ключ и применить' })
    expect((save as HTMLButtonElement).disabled).toBe(true)
    await userEvent.click(screen.getByLabelText(/отдельно разрешаю автоматическую проверку/i))
    expect((save as HTMLButtonElement).disabled).toBe(false)
    await userEvent.click(save)

    expect(calls).toContainEqual(expect.objectContaining({
      command: 'provider.save',
      payload: expect.objectContaining({
        freeAccessProbePolicy: 'on_first_use',
        acknowledgeProbePossibleCost: true
      })
    }))
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
