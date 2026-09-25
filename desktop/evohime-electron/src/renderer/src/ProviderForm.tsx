import { useCallback, useEffect, useState } from 'react'

import {
  PROVIDER_KINDS,
  PROVIDER_PROFILE_ENDPOINTS,
  PROVIDER_PROFILE_IDS,
  OLLAMA_DEFAULT_BASE_URL,
  type ModelTier,
  type ProviderKind,
  type ProviderProfileId,
  type ProviderSummary,
  type FreeAccessProbePolicy,
  type FreeAccessRoutingMode
} from '@shared/api'

import { useShellApi } from './shell-api'
import type { ConnectionState, CoreEvent } from '@shared/api'
import { OllamaModelDownloadPanel } from './OllamaModelDownloadPanel'
import { useProviderState } from './provider-state'

/**
 * Credentials surface.
 *
 * Everyday use needs two decisions: paste a key and pick free or paid models.
 * Provider and endpoint stay behind a disclosure so the common path is not
 * buried under them. The key itself is write-only from here: it goes to the
 * main process, which encrypts it with the OS key store and hands it to Core.
 */

const PROVIDER_LABELS: Record<ProviderKind, string> = {
  literouter: 'LiteRouter',
  openai_compatible: 'OpenAI API (Chat Completions)',
  openai_responses: 'OpenAI Responses API',
  ollama: 'Ollama (локально)'
}

const PROFILE_LABELS: Record<ProviderProfileId, string> = {
  openai: 'OpenAI',
  openrouter: 'OpenRouter',
  groq: 'Groq',
  gemini: 'Google Gemini',
  mistral: 'Mistral',
  cloudflare_workers_ai: 'Cloudflare Workers AI',
  nvidia_nim: 'NVIDIA NIM',
  cerebras: 'Cerebras',
  hugging_face: 'Hugging Face Inference Providers',
  custom: 'Другой OpenAI-compatible API'
}

const TIERS: readonly { readonly id: ModelTier; readonly label: string; readonly hint: string }[] = [
  { id: 'free', label: 'Бесплатные', hint: 'модели с суффиксом :free' },
  { id: 'paid', label: 'Платные', hint: 'списываются с баланса провайдера' }
]

type Status =
  | { readonly kind: 'idle' }
  | { readonly kind: 'saving' }
  | { readonly kind: 'saved'; readonly restarted: boolean; readonly action: 'provider' | 'settings' }
  | { readonly kind: 'failed'; readonly message: string }

export interface ProviderFormProps {
  readonly connection?: ConnectionState
  readonly events?: readonly CoreEvent[]
}

export function ProviderForm({ connection = 'starting', events = [] }: ProviderFormProps): React.JSX.Element {
  const api = useShellApi()
  const { summary, apply: applySummary } = useProviderState()
  const [provider, setProvider] = useState<ProviderKind>('literouter')
  const [apiKey, setApiKey] = useState('')
  const [model, setModel] = useState('')
  const [tier, setTier] = useState<ModelTier>('free')
  const [baseUrl, setBaseUrl] = useState('')
  const [profileId, setProfileId] = useState<ProviderProfileId>('openai')
  const [accountId, setAccountId] = useState('')
  const [probePolicy, setProbePolicy] = useState<FreeAccessProbePolicy>('disabled')
  const [routingMode, setRoutingMode] = useState<FreeAccessRoutingMode>('any')
  const [allowPaidFallback, setAllowPaidFallback] = useState(false)
  const [acknowledgeProbePossibleCost, setAcknowledgeProbePossibleCost] = useState(false)
  const [status, setStatus] = useState<Status>({ kind: 'idle' })
  const [catalogStatus, setCatalogStatus] = useState<string | null>(null)
  const [freeEvidenceStatus, setFreeEvidenceStatus] = useState<string | null>(null)
  const [probeStatus, setProbeStatus] = useState<string | null>(null)

  // Fields stay controlled even if a summary arrives with a missing member.
  const apply = useCallback((value: ProviderSummary) => {
    setProvider(PROVIDER_KINDS.includes(value.provider) ? value.provider : 'literouter')
    setModel(value.model ?? '')
    setTier(value.tier === 'paid' ? 'paid' : 'free')
    setBaseUrl(value.baseUrl ?? (value.provider === 'ollama' ? OLLAMA_DEFAULT_BASE_URL : ''))
    setProfileId(value.provider === 'openai_compatible' ? (value.profileId ?? 'openai') : 'custom')
    setAccountId(value.accountId ?? '')
    setProbePolicy(value.profiles?.[value.provider]?.freeAccessProbePolicy ?? 'disabled')
    setRoutingMode(value.freeAccessRoutingMode ?? 'any')
    setAllowPaidFallback(value.allowPaidFallback === true)
    setAcknowledgeProbePossibleCost(false)
  }, [])

  useEffect(() => {
    if (summary) apply(summary)
  }, [apply, summary])

  useEffect(() => {
    const event = [...events].reverse().find((item) => item.eventType === 'model.catalog')
    if (!event) {
      setCatalogStatus(null)
      return
    }
    const parsed = parseJson(event.payload)
    const projection = asRecord(parsed['provider_catalog'])
    const catalog = asRecord(projection?.['catalog'])
    const providerProjection = asRecord(projection?.['provider'])
    const freeAccess = asRecord(parsed['free_access'])
    const state = typeof catalog?.['state'] === 'string' ? catalog['state'] : null
    const failureCode = typeof catalog?.['failure_code'] === 'string'
      ? catalog['failure_code']
      : null
    const credentialStatus = typeof providerProjection?.['credential_status'] === 'string'
      ? providerProjection['credential_status']
      : null
    setCatalogStatus(state ? catalogStatusLabel(state, credentialStatus, failureCode) : null)
    setFreeEvidenceStatus(freeAccess ? freeAccessStatusLabel(freeAccess) : null)
  }, [events])

  useEffect(() => {
    const event = [...events].reverse().find((item) => item.eventType === 'free_access.probe')
    if (!event) return
    const result = parseJson(event.payload)
    const state = typeof result['state'] === 'string' ? result['state'] : 'unknown'
    const failure = typeof result['failure_code'] === 'string' ? result['failure_code'] : null
    if (result['strict_eligible'] === true) {
      setProbeStatus('Проверка подтвердила бесплатный доступ. Evidence сохранено на 24 часа.')
    } else if (state === 'paid_only') {
      setProbeStatus('Источник цены сообщил платное измерение. Строгий FreeOnly для модели закрыт.')
    } else {
      setProbeStatus(probeFailureLabel(failure))
    }
  }, [events])

  const selectProvider = useCallback(async (nextProvider: ProviderKind) => {
    if (!api || nextProvider === provider) return
    setProvider(nextProvider)
    setApiKey('')
    const profile = summary?.profiles?.[nextProvider]
    setModel(profile?.model ?? '')
    setTier(profile?.tier ?? 'free')
    setBaseUrl(profile?.baseUrl ?? (nextProvider === 'ollama' ? OLLAMA_DEFAULT_BASE_URL : ''))
    setProfileId(nextProvider === 'openai_compatible' ? (profile?.profileId ?? 'openai') : 'custom')
    setAccountId(profile?.accountId ?? '')
    setProbePolicy(profile?.freeAccessProbePolicy ?? 'disabled')
    setAcknowledgeProbePossibleCost(false)
    setStatus({ kind: 'saving' })

    const outcome = await api.invoke('provider.select', { provider: nextProvider })
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    applySummary(outcome.value.summary)
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted, action: 'provider' })
  }, [api, apply, applySummary, provider, summary])

  const selectProfile = useCallback((nextProfile: ProviderProfileId) => {
    setProfileId(nextProfile)
    setProbePolicy('disabled')
    setAcknowledgeProbePossibleCost(false)
    if (nextProfile === 'cloudflare_workers_ai') {
      setAccountId('')
      setBaseUrl('')
      return
    }
    setAccountId('')
    setBaseUrl(PROVIDER_PROFILE_ENDPOINTS[nextProfile] ?? '')
  }, [])

  const save = useCallback(async () => {
    if (!api) return
    setStatus({ kind: 'saving' })
    const outcome = await api.invoke('provider.save', {
      provider,
      apiKey,
      // The model is chosen per task in the composer, so it is not edited here.
      model,
      baseUrl,
      tier,
      freeAccessProbePolicy: probePolicy,
      acknowledgeProbePossibleCost,
      freeAccessRoutingMode: routingMode,
      allowPaidFallback: routingMode === 'prefer_free' && allowPaidFallback,
      ...(provider === 'openai_compatible' ? { profileId } : {}),
      ...(provider === 'openai_compatible' && profileId === 'cloudflare_workers_ai' ? { accountId } : {})
    })
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    applySummary(outcome.value.summary)
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted, action: 'settings' })
  }, [accountId, acknowledgeProbePossibleCost, allowPaidFallback, api, apiKey, apply, applySummary, baseUrl, model, probePolicy, profileId, provider, routingMode, tier])

  const clearKey = useCallback(async () => {
    if (!api) return
    setStatus({ kind: 'saving' })
    const outcome = await api.invoke('provider.clearKey', { provider })
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    applySummary(outcome.value.summary)
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted, action: 'settings' })
  }, [api, apply, applySummary, provider])

  const verifyFreeAccess = useCallback(async () => {
    if (!api || model.trim().length === 0) return
    const confirmed = window.confirm(
      'Core отправит один короткий запрос выбранной модели. Проверь цену и лимиты у провайдера: возможен расход кредитов или денег. Продолжить?'
    )
    if (!confirmed) return
    setProbeStatus('Проверка выполняется. Core сначала проверит источник цены, затем отправит bounded synthetic completion.')
    const outcome = await api.invoke('provider.verifyFreeAccess', {
      modelId: model,
      confirmPossibleCost: true
    })
    if (!outcome.ok) setProbeStatus(outcome.message)
  }, [api, model])

  const busy = status.kind === 'saving'
  const selectedProfile = summary?.profiles?.[provider]
  const configured = selectedProfile?.configured === true || (selectedProfile === undefined && summary?.provider === provider && summary.configured)
  const cloudflareAccountValid = /^[a-fA-F0-9]{32}$/.test(accountId.trim())
  const canSave = !busy
    && (provider === 'ollama' || apiKey.trim().length > 0 || configured)
    && (provider !== 'openai_compatible' || profileId !== 'cloudflare_workers_ai' || cloudflareAccountValid)
    && (!(probePolicy === 'on_first_use' || probePolicy === 'periodic_bounded') ||
      acknowledgeProbePossibleCost ||
      (probePolicy === selectedProfile?.freeAccessProbePolicy && apiKey.trim().length === 0 &&
        profileId === selectedProfile?.profileId && baseUrl === selectedProfile?.baseUrl && accountId === (selectedProfile?.accountId ?? '')))
  const displayBaseUrl = profileId === 'cloudflare_workers_ai' && cloudflareAccountValid
    ? `https://api.cloudflare.com/client/v4/accounts/${accountId.trim()}/ai/v1`
    : baseUrl

  return (
    <section className="shell__panel provider-form" aria-label="Ключ провайдера">
      <div className="settings-panel__heading">
        <div>
          <h2>Доступ к моделям</h2>
          <p className="shell__empty">
            {provider === 'ollama'
              ? 'Ollama работает локально. Ключ не нужен, модель можно скачать ниже.'
              : 'Ключ шифруется средствами Windows и хранится локально. Модель выбирается в чате.'}
          </p>
        </div>
        <span
          className={`settings-panel__state settings-panel__state--${configured ? 'ready' : 'offline'}`}
        >
          {provider === 'ollama' ? 'Локальный провайдер' : configured ? 'Ключ сохранён' : 'Ключ не задан'}
        </span>
      </div>

      {catalogStatus !== null ? <p className="provider-form__catalog-status" role="status">{catalogStatus}</p> : null}
      {freeEvidenceStatus !== null ? <p className="provider-form__catalog-status" role="status">{freeEvidenceStatus}</p> : null}

      <div className="provider-form__grid">
        <label htmlFor="provider-kind">
          Провайдер
          <select
            id="provider-kind"
            value={provider}
            onChange={(event) => void selectProvider(event.target.value as ProviderKind)}
            disabled={busy}
          >
            {PROVIDER_KINDS.map((kind) => (
              <option key={kind} value={kind}>{PROVIDER_LABELS[kind]}</option>
            ))}
          </select>
        </label>

        {provider !== 'ollama' ? (
          <label className="provider-form__key" htmlFor="provider-key">
            Ключ API
            <input
              id="provider-key"
              type="password"
              value={apiKey}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => {
                setApiKey(event.target.value)
                setAcknowledgeProbePossibleCost(false)
              }}
              placeholder={configured ? 'сохранён — введи новый, чтобы заменить' : 'sk-…'}
              disabled={busy}
            />
          </label>
        ) : null}

        {provider === 'openai_compatible' ? (
          <label htmlFor="provider-profile">
            Профиль провайдера
            <select
              id="provider-profile"
              value={profileId}
              onChange={(event) => selectProfile(event.target.value as ProviderProfileId)}
              disabled={busy}
            >
              {PROVIDER_PROFILE_IDS.map((id) => (
                <option key={id} value={id}>{PROFILE_LABELS[id]}</option>
              ))}
            </select>
          </label>
        ) : null}

        {provider === 'openai_compatible' && profileId === 'cloudflare_workers_ai' ? (
          <label htmlFor="provider-account-id">
            Cloudflare Account ID
            <input
              id="provider-account-id"
              value={accountId}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => {
                setAccountId(event.target.value)
                setProbePolicy('disabled')
                setAcknowledgeProbePossibleCost(false)
              }}
              placeholder="32 шестнадцатеричных символа"
              disabled={busy}
            />
          </label>
        ) : null}

        <label htmlFor="provider-url">
          Адрес API
          <input
            id="provider-url"
            value={displayBaseUrl}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setBaseUrl(event.target.value)
              setProbePolicy('disabled')
              setAcknowledgeProbePossibleCost(false)
            }}
            placeholder="по умолчанию провайдера"
            disabled={busy || (provider === 'openai_compatible' && profileId !== 'custom')}
          />
        </label>
      </div>

      {provider === 'openai_compatible' && profileId === 'cloudflare_workers_ai' ? (
        <p className="shell__empty">Нужен API token Cloudflare с разрешениями Workers AI Read и Workers AI Edit. Host и API path задаются профилем.</p>
      ) : null}

      {provider === 'ollama' ? (
        <OllamaModelDownloadPanel connection={connection} events={events} baseUrl={baseUrl} />
      ) : null}

      {provider !== 'ollama' ? <fieldset className="provider-form__tier">
        <legend>Какие модели показывать</legend>
        {TIERS.map((item) => (
          <label key={item.id}>
            <input
              type="radio"
              name="model-tier"
              value={item.id}
              checked={tier === item.id}
              onChange={() => setTier(item.id)}
              disabled={busy}
            />
            <span>{item.label}</span>
            <span className="provider-form__hint">{item.hint}</span>
          </label>
        ))}
      </fieldset> : null}

      <fieldset className="provider-form__tier">
        <legend>Проверка бесплатного доступа</legend>
        <label htmlFor="free-access-probe-policy">
          Как собирать данные
          <select
            id="free-access-probe-policy"
            value={probePolicy}
            onChange={(event) => {
              setProbePolicy(event.target.value as FreeAccessProbePolicy)
              setAcknowledgeProbePossibleCost(false)
            }}
            disabled={busy}
          >
            <option value="disabled">Выключена (ручную проверку можно запустить отдельно)</option>
            <option value="manual_only">Только ручная проверка</option>
            <option value="passive_only">Только наблюдать обычные запросы</option>
            <option value="on_first_use" disabled={provider !== 'openai_compatible' || profileId !== 'openrouter'}>Один раз при первом использовании</option>
            <option value="periodic_bounded" disabled={provider !== 'openai_compatible' || profileId !== 'openrouter'}>Повторять после истечения evidence</option>
          </select>
        </label>
        {probePolicy === 'on_first_use' || probePolicy === 'periodic_bounded' ? (
          <>
            <p className="provider-form__hint">Автоматическая проверка отправляет короткий synthetic запрос через OpenRouter. Возможен расход кредитов или денег.</p>
            <label>
              <input
                type="checkbox"
                checked={acknowledgeProbePossibleCost}
                onChange={(event) => setAcknowledgeProbePossibleCost(event.target.checked)}
                disabled={busy}
              />
              Я отдельно разрешаю автоматическую проверку с возможным расходом для этого профиля и ключа
            </label>
          </>
        ) : null}
      </fieldset>

      <fieldset className="provider-form__tier">
        <legend>Маршрутизация по evidence</legend>
        <label htmlFor="free-access-routing-mode">
          Режим
          <select
            id="free-access-routing-mode"
            value={routingMode}
            onChange={(event) => setRoutingMode(event.target.value as FreeAccessRoutingMode)}
            disabled={busy}
          >
            <option value="any">Обычная маршрутизация</option>
            <option value="prefer_free">Сначала подтверждённые бесплатные модели</option>
            <option value="free_only">Только подтверждённые бесплатные модели</option>
          </select>
        </label>
        <p className="provider-form__hint">Каталожная метка и суффикс :free не подтверждают доступ. FreeOnly блокирует неизвестные, устаревшие и платные маршруты.</p>
        {routingMode === 'prefer_free' ? (
          <label>
            <input
              type="checkbox"
              checked={allowPaidFallback}
              onChange={(event) => setAllowPaidFallback(event.target.checked)}
              disabled={busy}
            />
            Разрешить платный fallback, если подтверждённая бесплатная модель недоступна
          </label>
        ) : null}
      </fieldset>

      <div className="provider-form__actions">
        <button type="button" onClick={() => void save()} disabled={!canSave}>
          {provider === 'ollama' ? 'Сохранить параметры и применить' : 'Сохранить ключ и применить'}
        </button>
        {configured && provider !== 'ollama' ? (
          <button type="button" onClick={() => void clearKey()} disabled={busy}>
            Удалить ключ
          </button>
        ) : null}
      </div>

      {provider === 'openai_compatible' && profileId === 'openrouter' && configured ? (
        <div className="provider-form__catalog-status" aria-label="Проверка бесплатного доступа">
          <p>Разовая проверка OpenRouter: Core сверит цену модели и выполнит один короткий synthetic completion. Возможен расход.</p>
          <button type="button" onClick={() => void verifyFreeAccess()} disabled={busy || model.trim().length === 0}>
            Проверить текущую модель
          </button>
          {probeStatus ? <p role="status">{probeStatus}</p> : null}
        </div>
      ) : null}

      {status.kind === 'saved' ? (
        <p className={status.restarted ? 'provider-form__ok' : 'shell__reason'}>
          {status.restarted
            ? status.action === 'provider'
              ? 'Провайдер выбран и сохранён, Core перезапущен — подключение восстановится за пару секунд.'
              : 'Сохранено, Core перезапущен — подключение восстановится за пару секунд.'
            : status.action === 'provider'
              ? 'Провайдер сохранён, но Core не перезапустился. Перезапусти приложение вручную.'
              : 'Сохранено, но Core не перезапустился. Перезапусти приложение вручную.'}
        </p>
      ) : null}
      {status.kind === 'failed' ? (
        <p role="alert" className="shell__reason">{status.message}</p>
      ) : null}
    </section>
  )
}

function parseJson(payload: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
  } catch {
    return {}
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : null
}

function catalogStatusLabel(state: string, credentialStatus: string | null, failureCode: string | null): string {
  if (failureCode === 'model_not_found') return 'Core: модель не найдена у провайдера'
  if (credentialStatus === 'needs_credential') return 'Core: для каталога нужен ключ'
  if (credentialStatus === 'rejected') return 'Core: ключ провайдера отклонён'
  switch (state) {
    case 'fresh': return 'Core: каталог актуален'
    case 'stale': return 'Core: показан кэш каталога, маршрутизация остановлена до обновления'
    case 'unavailable': return 'Core: каталог временно недоступен'
    case 'discovery_unsupported': return 'Core: провайдер не поддерживает discovery'
    case 'credential_rejected': return 'Core: ключ провайдера отклонён'
    default: return 'Core: каталог ещё не проверен'
  }
}

function freeAccessStatusLabel(value: Record<string, unknown>): string {
  const state = typeof value['state'] === 'string' ? value['state'] : 'unknown'
  const advertised = typeof value['advertised_state'] === 'string' ? value['advertised_state'] : 'unknown'
  const allowance = typeof value['allowance'] === 'string' ? value['allowance'] : 'unknown'
  const activation = typeof value['activation'] === 'string' ? value['activation'] : 'unknown'
  const freshness = typeof value['freshness'] === 'string' ? value['freshness'] : 'unknown'
  const samples = typeof value['successful_sample_count'] === 'number' ? value['successful_sample_count'] : 0
  const confidence = typeof value['confidence_bps'] === 'number'
    ? `${(value['confidence_bps'] / 100).toFixed(1)}%`
    : 'не оценена'
  const observedAt = typeof value['observed_at_ms'] === 'number' && value['observed_at_ms'] > 0
    ? new Date(value['observed_at_ms']).toLocaleString()
    : 'нет'
  const limits = Array.isArray(value['limits'])
    ? value['limits'].slice(0, 16).map((item) => {
      const limit = asRecord(item)
      if (!limit) return null
      const scope = typeof limit['scope'] === 'string' ? limit['scope'] : 'unknown scope'
      const unit = typeof limit['unit'] === 'string' ? limit['unit'] : 'unknown unit'
      const source = typeof limit['source'] === 'string' ? limit['source'] : 'unknown source'
      return `${scope}/${unit}/${source}`
    }).filter((item): item is string => item !== null)
    : []
  return [
    value['strict_eligible'] === true ? 'FreeOnly: подтверждён' : 'FreeOnly: не подтверждён',
    `реклама ${advertised}`,
    `наблюдение ${state}`,
    `allowance ${allowance}`,
    `активация ${activation}`,
    `свежесть ${freshness}`,
    `образцов ${samples}`,
    `confidence ${confidence}`,
    `лимиты ${limits.length > 0 ? limits.join(', ') : 'не наблюдались'}`,
    `последняя запись ${observedAt}`
  ].join(' · ')
}

function probeFailureLabel(code: string | null): string {
  switch (code) {
    case 'billing_required': return 'Провайдер сообщил, что требуется billing. Строгий FreeOnly выключен.'
    case 'activation_required': return 'Провайдер требует активацию или смену плана. Бесплатность запроса не подтверждена.'
    case 'quota_rejected': return 'Квота исчерпана или действует cooldown; это не доказательство платного доступа.'
    case 'permission_denied': return 'Доступ запрещён, но причина не подтверждает необходимость оплаты.'
    case 'credential_rejected': return 'Провайдер отклонил ключ. Evidence остаётся Unknown.'
    case 'model_unavailable': return 'Провайдер не нашёл модель.'
    case 'empty_completion': return 'Ответ не содержал проверяемого результата.'
    case 'invalid_usage': return 'Usage ответа не прошёл bounded-проверку.'
    case 'cooldown': return 'Проверка уже выполняется или для этого credential-профиля ещё действует cooldown.'
    case 'consent_or_credential_required': return 'Не задано одноразовое согласие или opaque binding ключа.'
    case 'authority_unavailable': return 'Для этого профиля нет доверенного источника цены.'
    case 'provider_protocol_drift': return 'Ответ провайдера не соответствует проверяемому контракту.'
    case 'cancelled': return 'Проверка остановлена до сохранения строгого evidence.'
    case 'transport_failure': return 'Провайдер временно недоступен; повтори проверку позже.'
    default: return 'Бесплатный доступ не подтверждён. Evidence остаётся Unknown.'
  }
}
