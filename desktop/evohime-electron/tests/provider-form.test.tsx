// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, ProviderSummary, RendererCommand } from '../src/shared/api'
import { PROVIDER_PROFILE_IDS } from '../src/shared/api'
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
      allowPaidFallback: false,
      requestPacing: { enabled: true, intervalMs: 8000, freeIntervalMs: 18948 },
      modelRateLimits: { 'deepseek:free': { enabled: false, rpm: 0, rph: 0, rpd: 0 } }
    })
    await waitFor(() => expect(screen.getByText(/Core перезапущен/)).toBeTruthy())
    // The stored value is never echoed back into the form.
    expect(field.value).toBe('')
    expect(screen.getByText('Ключ сохранён')).toBeTruthy()
  })

  it('saves disabled per-model request limits by default and lets the user enable them', async () => {
    renderProviderForm()
    expect(await screen.findByLabelText('ID модели')).toBeTruthy()
    expect((screen.getByRole('checkbox', { name: /Включить лимиты для этой модели/i }) as HTMLInputElement).checked).toBe(false)
    await userEvent.type(screen.getByLabelText('Ключ API'), 'sk-rate-limit-test')
    await userEvent.click(screen.getByRole('checkbox', { name: /Включить лимиты для этой модели/i }))
    await userEvent.clear(screen.getByLabelText('RPM'))
    await userEvent.type(screen.getByLabelText('RPM'), '12')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))
    const save = calls.find((call) => call.command === 'provider.save')
    expect((save?.payload as { modelRateLimits: Record<string, unknown> }).modelRateLimits['deepseek:free']).toEqual({
      enabled: true, rpm: 12, rph: 0, rpd: 0
    })
  })

  it('offers per-model limits from the selected provider catalog instead of a text input', async () => {
    renderProviderForm([{
      sequenceId: 4,
      taskId: '',
      eventType: 'model.catalog',
      payload: JSON.stringify({ models: ['deepseek:free', 'qwen:free'] })
    }])

    const model = await screen.findByLabelText('ID модели') as HTMLSelectElement
    expect(model.tagName).toBe('SELECT')
    expect(Array.from(model.options).map((option) => option.value)).toEqual(['deepseek:free', 'qwen:free'])
    expect(screen.queryByRole('textbox', { name: 'ID модели' })).toBeNull()

    await userEvent.selectOptions(model, 'qwen:free')
    expect(model.value).toBe('qwen:free')
  })

  it('loads and saves per-provider request pacing', async () => {
    providerGetSummary = {
      provider: 'literouter',
      model: 'deepseek:free',
      baseUrl: '',
      tier: 'free',
      configured: true,
      profiles: {
        literouter: {
          model: 'deepseek:free', baseUrl: '', tier: 'free', configured: true,
          requestPacing: { enabled: true, intervalMs: 1234, freeIntervalMs: 4321 }
        },
        openai_compatible: { model: '', baseUrl: '', tier: 'free', configured: false },
        openai_responses: { model: '', baseUrl: '', tier: 'free', configured: false },
        ollama: { model: '', baseUrl: 'http://127.0.0.1:11434/v1', tier: 'free', configured: true }
      }
    }
    renderProviderForm()

    const interval = await screen.findByLabelText('Интервал для платных моделей, мс') as HTMLInputElement
    const freeInterval = screen.getByLabelText('Интервал для моделей :free, мс') as HTMLInputElement
    await waitFor(() => expect(interval.value).toBe('1234'))
    expect(freeInterval.value).toBe('4321')

    await userEvent.clear(interval)
    await userEvent.type(interval, '2500')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    expect(calls).toContainEqual({
      command: 'provider.save',
      payload: expect.objectContaining({
        provider: 'literouter',
        requestPacing: { enabled: true, intervalMs: 2500, freeIntervalMs: 4321 }
      })
    })
  })

  it('disables saving when a pacing interval is not a bounded integer', async () => {
    renderProviderForm()

    await userEvent.type(await screen.findByLabelText('Ключ API'), 'sk-test')
    const save = screen.getByRole('button', { name: 'Сохранить ключ и применить' }) as HTMLButtonElement
    expect(save.disabled).toBe(false)

    const interval = screen.getByLabelText('Интервал для платных моделей, мс') as HTMLInputElement
    await userEvent.clear(interval)
    await userEvent.type(interval, '3600001')

    expect(save.disabled).toBe(true)
  })

  it('keeps the provider settings block free of a separate coding-engine switch', async () => {
    renderProviderForm()
    expect(await screen.findByLabelText('Провайдер')).toBeTruthy()
    expect(screen.queryByLabelText('Движок coding-задач')).toBeNull()
  })

  it('lists every vendor directly in one provider selector', async () => {
    renderProviderForm()

    const provider = (await screen.findByLabelText('Провайдер')) as HTMLSelectElement
    const values = Array.from(provider.options, (option) => option.value)
    expect(values).toEqual(['literouter', ...PROVIDER_PROFILE_IDS, 'openai_responses', 'ollama'])
    expect(screen.queryByLabelText('Профиль провайдера')).toBeNull()
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

  it('restores a separately saved compatible vendor key status when switching vendors', async () => {
    providerGetSummary = {
      provider: 'openai_compatible',
      model: 'llama-groq',
      baseUrl: 'https://api.groq.com/openai/v1',
      tier: 'paid',
      configured: true,
      profileId: 'groq',
      profiles: {
        literouter: { model: '', baseUrl: '', tier: 'free', configured: false },
        openai_compatible: {
          model: 'llama-groq', baseUrl: 'https://api.groq.com/openai/v1', tier: 'paid', configured: true, profileId: 'groq'
        },
        openai_responses: { model: '', baseUrl: '', tier: 'free', configured: false },
        ollama: { model: '', baseUrl: 'http://127.0.0.1:11434/v1', tier: 'free', configured: true }
      },
      compatibleProfiles: {
        groq: { model: 'llama-groq', baseUrl: 'https://api.groq.com/openai/v1', tier: 'paid', configured: true, profileId: 'groq' },
        deepseek: { model: 'deepseek-chat', baseUrl: 'https://api.deepseek.com/v1', tier: 'paid', configured: true, profileId: 'deepseek' }
      }
    }
    selectOutcome = ok({
      summary: {
        provider: 'openai_compatible',
        model: 'deepseek-chat',
        baseUrl: 'https://api.deepseek.com/v1',
        tier: 'paid',
        configured: true,
        profileId: 'deepseek',
        profiles: (providerGetSummary as unknown as ProviderSummary).profiles,
        compatibleProfiles: (providerGetSummary as unknown as ProviderSummary).compatibleProfiles
      },
      restarted: true
    })
    renderProviderForm()

    const provider = await screen.findByLabelText('Провайдер')
    await userEvent.selectOptions(provider, 'deepseek')

    await waitFor(() => expect(calls).toContainEqual({
      command: 'provider.select',
      payload: { provider: 'openai_compatible', profileId: 'deepseek' }
    }))
    expect(await screen.findByText('Ключ сохранён')).toBeTruthy()
    expect((screen.getByLabelText('Адрес API') as HTMLInputElement).value).toBe('https://api.deepseek.com/v1')
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

    await userEvent.selectOptions(await screen.findByLabelText('Провайдер'), 'cloudflare_workers_ai')
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
    const openrouterProfiles = {
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
    providerGetSummary = {
      provider: 'literouter',
      model: '',
      baseUrl: '',
      tier: 'free',
      configured: false,
      profiles: openrouterProfiles
    }
    selectOutcome = ok({
      summary: {
        provider: 'openai_compatible',
        model: 'author/model:free',
        baseUrl: 'https://openrouter.ai/api/v1',
        tier: 'free',
        configured: true,
        profileId: 'openrouter',
        profiles: openrouterProfiles
      },
      restarted: true
    })
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true)
    renderProviderForm()

    await userEvent.selectOptions(await screen.findByLabelText('Провайдер'), 'openrouter')
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

  it('keeps an unconfigured provider selected as a local draft instead of calling provider.select', async () => {
    selectOutcome = { ok: false, code: 'invalid-payload', message: 'Сначала настрой ключ выбранного провайдера.' }
    renderProviderForm()

    const select = (await screen.findByLabelText('Провайдер')) as HTMLSelectElement
    await userEvent.selectOptions(select, 'openai_responses')

    expect(select.value).toBe('openai_responses')
    expect(calls.some((call) => call.command === 'provider.select')).toBe(false)
    expect(screen.queryByRole('alert')).toBeNull()

    await userEvent.type(screen.getByLabelText('Ключ API'), 'sk-responses-test')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    await waitFor(() => expect(calls).toContainEqual({
      command: 'provider.save',
      payload: expect.objectContaining({
        provider: 'openai_responses',
        apiKey: 'sk-responses-test'
      })
    }))
  })

  it('defaults a new OpenRouter profile to automatic first-use checks and requires cost consent', async () => {
    selectOutcome = ok({
      summary: {
        provider: 'openai_compatible',
        model: 'author/model:free',
        baseUrl: 'https://api.openai.com/v1',
        tier: 'free',
        configured: true,
        profileId: 'openai',
        profiles: {
          literouter: { model: '', baseUrl: '', tier: 'free', configured: false },
          openai_compatible: {
            model: 'author/model:free', baseUrl: 'https://api.openai.com/v1', tier: 'free',
            configured: true, profileId: 'openai', freeAccessProbePolicy: 'disabled'
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
    await userEvent.selectOptions(await screen.findByLabelText('Провайдер'), 'openrouter')
    expect((screen.getByLabelText('Как собирать данные') as HTMLSelectElement).value).toBe('on_first_use')
    const save = screen.getByRole('button', { name: 'Сохранить ключ и применить' })
    expect((save as HTMLButtonElement).disabled).toBe(true)
    await userEvent.type(screen.getByLabelText('Ключ API'), 'sk-openrouter')
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

  it('preserves an existing OpenRouter policy when returning to its saved profile', async () => {
    providerGetSummary = {
      provider: 'openai_compatible',
      model: 'author/model:free',
      baseUrl: 'https://openrouter.ai/api/v1',
      tier: 'free',
      configured: true,
      profileId: 'openrouter',
      profiles: {
        openai_compatible: {
          model: 'author/model:free', baseUrl: 'https://openrouter.ai/api/v1', tier: 'free',
          configured: true, profileId: 'openrouter', freeAccessProbePolicy: 'disabled'
        }
      }
    }
    renderProviderForm()

    const profile = await screen.findByLabelText('Провайдер')
    await userEvent.selectOptions(profile, 'openai')
    await userEvent.selectOptions(profile, 'openrouter')

    expect((screen.getByLabelText('Как собирать данные') as HTMLSelectElement).value).toBe('disabled')
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

  it('shows the free/paid tier selector only for LiteRouter', async () => {
    renderProviderForm()

    const profile = await screen.findByLabelText('Провайдер')
    expect(screen.queryByText('Какие модели показывать')).toBeTruthy()

    await userEvent.selectOptions(profile, 'deepseek')
    expect(screen.queryByText('Какие модели показывать')).toBeNull()

    await userEvent.selectOptions(profile, 'openai_responses')
    expect(screen.queryByText('Какие модели показывать')).toBeNull()

    await userEvent.selectOptions(profile, 'literouter')
    expect(screen.queryByText('Какие модели показывать')).toBeTruthy()
  })

  it('hides free-access evidence and pacing controls that cannot affect the selected provider', async () => {
    renderProviderForm()

    const provider = await screen.findByLabelText('Провайдер')
    // LiteRouter owns the tier window but has no pricing authority for probes.
    expect(screen.getByText('Пауза между запросами')).toBeTruthy()
    expect(screen.queryByText('Проверка бесплатного доступа')).toBeNull()
    expect(screen.queryByText('Маршрутизация по evidence')).toBeNull()

    await userEvent.selectOptions(provider, 'deepseek')
    expect(screen.queryByText('Проверка бесплатного доступа')).toBeNull()
    expect(screen.queryByText('Маршрутизация по evidence')).toBeNull()
    expect(screen.queryByText('Пауза между запросами')).toBeNull()

    await userEvent.selectOptions(provider, 'openrouter')
    expect(await screen.findByText('Проверка бесплатного доступа')).toBeTruthy()
    expect(screen.getByText('Маршрутизация по evidence')).toBeTruthy()
  })

  it('keeps a non-default routing mode visible so it can be undone', async () => {
    providerGetSummary = {
      provider: 'literouter',
      model: '',
      baseUrl: '',
      tier: 'free',
      configured: false,
      freeAccessRoutingMode: 'free_only'
    }
    renderProviderForm()

    expect(await screen.findByText('Маршрутизация по evidence')).toBeTruthy()
    expect((screen.getByLabelText('Режим') as HTMLSelectElement).value).toBe('free_only')
  })

  it('keeps an explicitly enabled pause visible for a non-LiteRouter provider', async () => {
    providerGetSummary = {
      provider: 'openai_compatible',
      model: 'deepseek-chat',
      baseUrl: 'https://api.deepseek.com/v1',
      tier: 'paid',
      configured: true,
      profileId: 'deepseek',
      profiles: {
        openai_compatible: {
          model: 'deepseek-chat', baseUrl: 'https://api.deepseek.com/v1', tier: 'paid',
          configured: true, profileId: 'deepseek',
          requestPacing: { enabled: true, intervalMs: 4321, freeIntervalMs: 18948 }
        }
      }
    }
    renderProviderForm()

    const interval = await screen.findByLabelText('Интервал для платных моделей, мс') as HTMLInputElement
    await waitFor(() => expect(interval.value).toBe('4321'))
    // The tier-window explanation belongs to LiteRouter only.
    expect(screen.queryByText(/LiteRouter ограничивает/)).toBeNull()
  })

  it('provider_form_uses_schema', async () => {
    renderProviderForm()

    const provider = await screen.findByLabelText('Провайдер')
    await userEvent.selectOptions(provider, 'cloudflare_workers_ai')
    expect(await screen.findByLabelText('Cloudflare Account ID')).toBeTruthy()
    expect(screen.getByLabelText('Ключ API')).toBeTruthy()
    expect(screen.getByRole('group', { name: 'Лимиты запросов модели' })).toBeTruthy()

    await userEvent.selectOptions(provider, 'ollama')
    expect(screen.queryByLabelText('Ключ API')).toBeNull()
    expect(screen.queryByLabelText('Cloudflare Account ID')).toBeNull()
    expect(screen.queryByRole('group', { name: 'Лимиты запросов модели' })).toBeNull()
  })

  it('provider_form_saves_model_limits', async () => {
    renderProviderForm([{
      sequenceId: 10,
      taskId: '',
      eventType: 'model.catalog',
      payload: JSON.stringify({ models: ['deepseek:free', 'qwen:free'] })
    }])

    expect(await screen.findByLabelText('ID модели')).toBeTruthy()
    await userEvent.type(screen.getByLabelText('Ключ API'), 'sk-multi-model-limits')

    await userEvent.click(screen.getByRole('button', { name: 'Добавить модель' }))

    const modelSelects = screen.getAllByLabelText('ID модели') as HTMLSelectElement[]
    expect(modelSelects).toHaveLength(2)
    expect(modelSelects[0].value).toBe('deepseek:free')
    expect(modelSelects[1].value).toBe('qwen:free')

    const checkboxes = screen.getAllByRole('checkbox', { name: /Включить лимиты для этой модели/i })
    const rpmInputs = screen.getAllByLabelText('RPM')
    const rpdInputs = screen.getAllByLabelText('RPD')

    await userEvent.click(checkboxes[0])
    await userEvent.clear(rpmInputs[0])
    await userEvent.type(rpmInputs[0], '10')

    await userEvent.click(checkboxes[1])
    await userEvent.clear(rpmInputs[1])
    await userEvent.type(rpmInputs[1], '25')
    await userEvent.clear(rpdInputs[1])
    await userEvent.type(rpdInputs[1], '500')

    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ и применить' }))

    const save = calls.find((call) => call.command === 'provider.save')
    expect((save?.payload as { modelRateLimits: Record<string, unknown> }).modelRateLimits).toEqual({
      'deepseek:free': { enabled: true, rpm: 10, rph: 0, rpd: 0 },
      'qwen:free': { enabled: true, rpm: 25, rph: 0, rpd: 500 }
    })
  })
})
