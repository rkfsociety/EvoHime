import { useCallback, useEffect, useRef, useState } from 'react'

import {
  PROVIDER_KINDS,
  PROVIDER_PROFILE_ENDPOINTS,
  PROVIDER_PROFILE_IDS,
  OLLAMA_DEFAULT_BASE_URL,
  REQUEST_PACING_MAX_INTERVAL_MS,
  defaultRequestPacing,
  type ConnectionState,
  type CoreEvent,
  type FreeAccessProbePolicy,
  type FreeAccessRoutingMode,
  type ModelRateLimitSettings,
  type ModelTier,
  type ProviderKind,
  type ProviderModelRateLimits,
  type ProviderProfileId,
  type ProviderProfileSummary,
  type ProviderSummary,
  type RequestPacingSettings
} from '@shared/api'

import { ModelSettingsBlock, type ModelLimitDraft } from './ModelSettingsBlock'
import { ProviderBaseFields } from './ProviderBaseFields'
import { ProviderSpecificSettings } from './ProviderSpecificSettings'
import { translate, formatDateTime } from './i18n'
import { PROVIDER_PROFILE_LABELS } from './provider-labels'
import { getProviderSchema } from './provider-schema'
import { useProviderState } from './provider-state'
import { useShellApi } from './shell-api'

/**
 * Credentials surface.
 *
 * Everyday use needs two decisions: paste a key and pick free or paid models.
 * Provider and endpoint stay behind a disclosure so the common path is not
 * buried under them. The key itself is write-only from here: it goes to the
 * main process, which encrypts it with the OS key store and hands it to Core.
 */

/** A single provider choice: transport-level provider or fixed vendor profile. */
type ProviderChoice = Exclude<ProviderKind, 'openai_compatible'> | ProviderProfileId

function profileProbePolicy(
  summary: ProviderSummary | ProviderProfileSummary | undefined
): FreeAccessProbePolicy | undefined {
  return summary !== undefined && 'freeAccessProbePolicy' in summary
    ? summary.freeAccessProbePolicy
    : undefined
}

const PROVIDER_CHOICES: readonly { readonly value: ProviderChoice; readonly label: string }[] = [
  { value: 'literouter', label: 'LiteRouter' },
  ...PROVIDER_PROFILE_IDS.map((profileId) => ({
    value: profileId,
    label: PROVIDER_PROFILE_LABELS[profileId]
  })),
  { value: 'openai_responses', label: 'OpenAI Responses API' },
  { value: 'ollama', label: 'Ollama (локально)' }
]

function isProviderProfileChoice(choice: ProviderChoice): choice is ProviderProfileId {
  return (PROVIDER_PROFILE_IDS as readonly string[]).includes(choice)
}

/** Parses a millisecond interval typed by the user, or reports it invalid. */
function parsePacingInterval(value: string): number | null {
  const trimmed = value.trim()
  if (!/^\d+$/.test(trimmed)) return null
  const parsed = Number(trimmed)
  return Number.isSafeInteger(parsed) && parsed <= REQUEST_PACING_MAX_INTERVAL_MS ? parsed : null
}

function parseModelCap(value: string): number | null {
  const trimmed = value.trim()
  if (!/^\d+$/.test(trimmed)) return null
  const parsed = Number(trimmed)
  return Number.isSafeInteger(parsed) && parsed <= 10_000 ? parsed : null
}

function parseModelPrice(value: string): number | null {
  const trimmed = value.trim().replace(',', '.')
  if (trimmed.length === 0) return 0
  const parsed = Number(trimmed)
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 10_000 ? parsed : null
}

function pacingOrDefault(value: RequestPacingSettings | undefined, provider: ProviderKind): RequestPacingSettings {
  return value ?? defaultRequestPacing(provider)
}

function buildModelLimitDrafts(
  limits: ProviderModelRateLimits | undefined,
  activeModel: string,
  nextKey: () => string
): ModelLimitDraft[] {
  const entries: ModelLimitDraft[] = []
  const seen = new Set<string>()
  const trimmedActive = activeModel.trim()

  if (limits) {
    for (const [modelId, settings] of Object.entries(limits)) {
      const trimmed = modelId.trim()
      if (!trimmed || seen.has(trimmed)) continue
      seen.add(trimmed)
      entries.push({
        key: nextKey(),
        modelId: trimmed,
        enabled: settings.enabled === true,
        rpm: String(settings.rpm ?? 0),
        rph: String(settings.rph ?? 0),
        rpd: String(settings.rpd ?? 0),
        pricingEnabled: settings.pricing !== undefined,
        inputUsdPer1M: String(settings.pricing?.inputUsdPer1M ?? 0),
        cachedInputUsdPer1M: String(settings.pricing?.cachedInputUsdPer1M ?? 0),
        outputUsdPer1M: String(settings.pricing?.outputUsdPer1M ?? 0)
      })
    }
  }

  if (trimmedActive && !seen.has(trimmedActive)) {
    entries.unshift({
      key: nextKey(),
      modelId: trimmedActive,
      enabled: false,
      rpm: '0',
      rph: '0',
      rpd: '0',
      pricingEnabled: false,
      inputUsdPer1M: '0',
      cachedInputUsdPer1M: '0',
      outputUsdPer1M: '0'
    })
  }

  if (entries.length === 0) {
    entries.push({
      key: nextKey(),
      modelId: '',
      enabled: false,
      rpm: '0',
      rph: '0',
      rpd: '0',
      pricingEnabled: false,
      inputUsdPer1M: '0',
      cachedInputUsdPer1M: '0',
      outputUsdPer1M: '0'
    })
  }

  return entries
}

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
  const [pacingEnabled, setPacingEnabled] = useState(true)
  const [pacingInterval, setPacingInterval] = useState('8000')
  const [pacingFreeInterval, setPacingFreeInterval] = useState('18948')
  const [catalogModels, setCatalogModels] = useState<readonly string[]>([])
  const cardKeySeq = useRef(0)
  const nextCardKey = useCallback(() => {
    cardKeySeq.current += 1
    return `m-${cardKeySeq.current}`
  }, [])
  const [modelLimitEntries, setModelLimitEntries] = useState<ModelLimitDraft[]>(() =>
    buildModelLimitDrafts(undefined, '', () => 'm-0')
  )
  const [acknowledgeProbePossibleCost, setAcknowledgeProbePossibleCost] = useState(false)
  const [status, setStatus] = useState<Status>({ kind: 'idle' })
  const [catalogStatus, setCatalogStatus] = useState<string | null>(null)
  const [freeEvidenceStatus, setFreeEvidenceStatus] = useState<string | null>(null)
  const [probeStatus, setProbeStatus] = useState<string | null>(null)
  // A vendor choice is local until the user saves its key. While it differs
  // from the stored profile, incoming snapshots must not overwrite the form.
  const pendingSelection = useRef(false)

  const schema = getProviderSchema(provider, profileId)

  const selectedProfile = provider === 'openai_compatible'
    ? summary?.compatibleProfiles?.[profileId]
      ?? (summary?.profiles?.openai_compatible?.profileId === profileId ? summary.profiles.openai_compatible : undefined)
      ?? (summary?.provider === provider && summary.profileId === profileId ? summary : undefined)
    : summary?.profiles?.[provider]

  const applyPacing = useCallback((pacing: RequestPacingSettings) => {
    setPacingEnabled(pacing.enabled)
    setPacingInterval(String(pacing.intervalMs))
    setPacingFreeInterval(String(pacing.freeIntervalMs))
  }, [])

  const apply = useCallback((value: ProviderSummary) => {
    const nextProvider = PROVIDER_KINDS.includes(value.provider) ? value.provider : 'literouter'
    const nextProfileId = nextProvider === 'openai_compatible' ? (value.profileId ?? 'openai') : 'custom'
    const nextSchema = getProviderSchema(nextProvider, nextProfileId)

    setProvider(nextProvider)
    setModel(value.model ?? '')
    setTier(value.tier === 'paid' ? 'paid' : 'free')
    setBaseUrl(value.baseUrl ?? (nextSchema.supportsLocalModelDownload ? OLLAMA_DEFAULT_BASE_URL : ''))
    setProfileId(nextProfileId)
    setAccountId(value.accountId ?? '')
    setProbePolicy(value.profiles?.[value.provider]?.freeAccessProbePolicy ?? nextSchema.defaultProbePolicy)
    setRoutingMode(value.freeAccessRoutingMode ?? 'any')
    setAllowPaidFallback(value.allowPaidFallback === true)
    applyPacing(value.requestPacing ?? value.profiles?.[value.provider]?.requestPacing ?? defaultRequestPacing(nextProvider))
    setModelLimitEntries(
      buildModelLimitDrafts(
        value.modelRateLimits ?? value.profiles?.[value.provider]?.modelRateLimits,
        value.model ?? '',
        nextCardKey
      )
    )
    setAcknowledgeProbePossibleCost(false)
  }, [applyPacing, nextCardKey])

  useEffect(() => {
    if (summary && !pendingSelection.current) apply(summary)
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
    const expectedProvider = provider === 'openai_compatible' ? profileId : provider
    const catalogProvider = typeof providerProjection?.['profile_id'] === 'string'
      ? providerProjection['profile_id']
      : typeof providerProjection?.['id'] === 'string' ? providerProjection['id'] : null
    if (catalogProvider !== null && catalogProvider !== expectedProvider) {
      setCatalogModels([])
      return
    }
    const models = Array.isArray(parsed['models'])
      ? parsed['models'].filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
      : []
    setCatalogModels([...new Set(models)])
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
  }, [events, profileId, provider])

  useEffect(() => {
    if (!api || !['connected', 'replaying', 'resyncing'].includes(connection)) return
    setCatalogModels([])
    void api.invoke('core.listModelCatalog', { mode: tier })
  }, [api, connection, provider, tier])

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

  const selectChoice = useCallback(async (choice: ProviderChoice) => {
    if (!api) return
    const nextProvider: ProviderKind = isProviderProfileChoice(choice) ? 'openai_compatible' : choice
    const nextProfileId: ProviderProfileId = isProviderProfileChoice(choice) ? choice : 'custom'
    if (nextProvider === provider && nextProfileId === profileId) return

    const nextSchema = getProviderSchema(nextProvider, nextProfileId)
    const saved = nextProvider === 'openai_compatible'
      ? summary?.compatibleProfiles?.[nextProfileId]
        ?? (summary?.profiles?.openai_compatible?.profileId === nextProfileId ? summary.profiles.openai_compatible : undefined)
        ?? (summary?.provider === nextProvider && summary.profileId === nextProfileId ? summary : undefined)
      : summary?.profiles?.[nextProvider]
    const savedMatches = nextProvider !== 'openai_compatible' || saved !== undefined
    const nextConfigured = savedMatches && (
      saved?.configured === true
      || (saved === undefined && summary?.provider === nextProvider && summary.configured &&
        (nextProvider !== 'openai_compatible' || summary.profileId === nextProfileId))
    )
    pendingSelection.current = true

    setProvider(nextProvider)
    setProfileId(nextProfileId)
    setApiKey('')
    setModel(savedMatches ? (saved?.model ?? '') : '')
    setTier(savedMatches ? (saved?.tier ?? nextSchema.defaultTier) : nextSchema.defaultTier)
    setBaseUrl(
      savedMatches
        ? (saved?.baseUrl ?? (nextSchema.supportsLocalModelDownload ? OLLAMA_DEFAULT_BASE_URL : ''))
        : (PROVIDER_PROFILE_ENDPOINTS[nextProfileId] ?? '')
    )
    setAccountId(savedMatches ? (saved?.accountId ?? '') : '')
    setProbePolicy(
      savedMatches
        ? (profileProbePolicy(saved) ?? nextSchema.defaultProbePolicy)
        : nextSchema.defaultProbePolicy
    )
    applyPacing(pacingOrDefault(saved?.requestPacing, nextProvider))
    setModelLimitEntries(
      buildModelLimitDrafts(
        savedMatches ? saved?.modelRateLimits : undefined,
        savedMatches ? (saved?.model ?? '') : '',
        nextCardKey
      )
    )
    setAcknowledgeProbePossibleCost(false)

    // Selecting an unconfigured provider or switching vendor inside the
    // OpenAI-compatible transport stays a local draft until "Save" persists
    // the profile and its credential.
    if (!nextConfigured || (nextProvider === provider && nextProvider !== 'openai_compatible')) {
      if (savedMatches && summary?.provider === nextProvider) pendingSelection.current = false
      setStatus({ kind: 'idle' })
      return
    }

    setStatus({ kind: 'saving' })
    const outcome = await api.invoke('provider.select', {
      provider: nextProvider,
      ...(nextProvider === 'openai_compatible' ? { profileId: nextProfileId } : {})
    })
    if (!outcome.ok) {
      pendingSelection.current = false
      if (summary) apply(summary)
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    applySummary(outcome.value.summary)
    const activeProfileId = nextProvider === 'openai_compatible'
      ? outcome.value.summary.profileId
      : nextProfileId
    if (activeProfileId === nextProfileId) {
      pendingSelection.current = false
      apply(outcome.value.summary)
    }
    setStatus({ kind: 'saved', restarted: outcome.value.restarted, action: 'provider' })
  }, [api, apply, applyPacing, applySummary, nextCardKey, profileId, provider, summary])

  const updateModelLimitEntry = useCallback(
    (key: string, patch: Partial<Omit<ModelLimitDraft, 'key'>>) => {
      setModelLimitEntries((current) =>
        current.map((entry) => (entry.key === key ? { ...entry, ...patch } : entry))
      )
    },
    []
  )

  const addModelLimitEntry = useCallback(() => {
    setModelLimitEntries((current) => {
      const used = new Set(current.map((item) => item.modelId.trim()).filter(Boolean))
      const suggested = catalogModels.find((id) => !used.has(id)) ?? ''
      return [
        ...current,
        {
          key: nextCardKey(),
          modelId: suggested,
          enabled: false,
          rpm: '0',
          rph: '0',
          rpd: '0',
          pricingEnabled: false,
          inputUsdPer1M: '0',
          cachedInputUsdPer1M: '0',
          outputUsdPer1M: '0'
        }
      ]
    })
  }, [catalogModels, nextCardKey])

  const removeModelLimitEntry = useCallback((key: string) => {
    setModelLimitEntries((current) => {
      const next = current.filter((item) => item.key !== key)
      return next.length > 0 ? next : current
    })
  }, [])

  const save = useCallback(async () => {
    if (!api) return
    const intervalMs = parsePacingInterval(pacingInterval)
    const freeIntervalMs = parsePacingInterval(pacingFreeInterval)
    if (intervalMs === null || freeIntervalMs === null) {
      setStatus({ kind: 'failed', message: 'Интервал паузы должен быть целым числом от 0 до 3600000 мс.' })
      return
    }

    const nextModelRateLimits: Record<string, ModelRateLimitSettings> = {}
    if (schema.supportsModelLimits) {
      for (const entry of modelLimitEntries) {
        const trimmedModel = entry.modelId.trim()
        if (!trimmedModel) continue
        const rpm = parseModelCap(entry.rpm)
        const rph = parseModelCap(entry.rph)
        const rpd = parseModelCap(entry.rpd)
        if (rpm === null || rph === null || rpd === null) {
          setStatus({ kind: 'failed', message: 'Лимиты RPM/RPH/RPD должны быть целыми числами от 0 до 10000.' })
          return
        }
        let pricing: ModelRateLimitSettings['pricing']
        if (entry.pricingEnabled) {
          const inputUsdPer1M = parseModelPrice(entry.inputUsdPer1M)
          const cachedInputUsdPer1M = parseModelPrice(entry.cachedInputUsdPer1M)
          const outputUsdPer1M = parseModelPrice(entry.outputUsdPer1M)
          if (inputUsdPer1M === null || cachedInputUsdPer1M === null || outputUsdPer1M === null) {
            setStatus({ kind: 'failed', message: 'Стоимость токенов должна быть числом от 0 до 10000 USD за 1 млн.' })
            return
          }
          pricing = { inputUsdPer1M, cachedInputUsdPer1M, outputUsdPer1M }
        }
        nextModelRateLimits[trimmedModel] = {
          enabled: entry.enabled,
          rpm,
          rph,
          rpd,
          ...(pricing ? { pricing } : {})
        }
      }
    }

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
      requestPacing: { enabled: pacingEnabled, intervalMs, freeIntervalMs },
      modelRateLimits: schema.supportsModelLimits
        ? nextModelRateLimits
        : (selectedProfile?.modelRateLimits ?? {}),
      ...(provider === 'openai_compatible' ? { profileId } : {}),
      ...(schema.requiresAccountId ? { accountId } : {})
    })
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    pendingSelection.current = false
    applySummary(outcome.value.summary)
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted, action: 'settings' })
  }, [
    accountId,
    acknowledgeProbePossibleCost,
    allowPaidFallback,
    api,
    apiKey,
    apply,
    applySummary,
    baseUrl,
    model,
    modelLimitEntries,
    pacingEnabled,
    pacingFreeInterval,
    pacingInterval,
    probePolicy,
    profileId,
    provider,
    routingMode,
    schema.requiresAccountId,
    schema.supportsModelLimits,
    selectedProfile,
    tier
  ])

  const clearKey = useCallback(async () => {
    if (!api) return
    setStatus({ kind: 'saving' })
    const outcome = await api.invoke('provider.clearKey', {
      provider,
      ...(provider === 'openai_compatible' ? { profileId } : {})
    })
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    pendingSelection.current = false
    applySummary(outcome.value.summary)
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted, action: 'settings' })
  }, [api, apply, applySummary, profileId, provider])

  const verifyFreeAccess = useCallback(async () => {
    if (!api || model.trim().length === 0) return
    const confirmed = window.confirm(
      translate('Core отправит один короткий запрос выбранной модели. Проверь цену и лимиты у провайдера: возможен расход кредитов или денег. Продолжить?')
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
  const selection: ProviderChoice = provider === 'openai_compatible' ? profileId : provider
  const profileMatches = provider !== 'openai_compatible' || selectedProfile?.profileId === profileId
  const configured = profileMatches && (selectedProfile?.configured === true || (
    selectedProfile === undefined && summary?.provider === provider && summary.configured &&
    (provider !== 'openai_compatible' || summary.profileId === profileId)
  ))
  const cloudflareAccountValid = /^[a-fA-F0-9]{32}$/.test(accountId.trim())
  const pacingValid = parsePacingInterval(pacingInterval) !== null
    && parsePacingInterval(pacingFreeInterval) !== null
  const canSave = !busy
    && pacingValid
    && (!schema.requiresApiKey || apiKey.trim().length > 0 || configured)
    && (!schema.requiresAccountId || cloudflareAccountValid)
    && (!(probePolicy === 'on_first_use' || probePolicy === 'periodic_bounded') ||
      acknowledgeProbePossibleCost ||
      (probePolicy === profileProbePolicy(selectedProfile) && apiKey.trim().length === 0 &&
        profileId === selectedProfile?.profileId && baseUrl === selectedProfile?.baseUrl && accountId === (selectedProfile?.accountId ?? '')))
  const displayBaseUrl = schema.requiresAccountId && cloudflareAccountValid
    ? `https://api.cloudflare.com/client/v4/accounts/${accountId.trim()}/ai/v1`
    : baseUrl
  const pacingDefaults = defaultRequestPacing(provider)
  const pacingIsDefault = !pacingEnabled
    && pacingInterval === String(pacingDefaults.intervalMs)
    && pacingFreeInterval === String(pacingDefaults.freeIntervalMs)

  const badgeText = configured
    ? (schema.configuredBadge ?? 'Ключ сохранён')
    : (schema.unconfiguredBadge ?? 'Ключ не задан')

  return (
    <section className="shell__panel provider-form" aria-label={translate('Ключ провайдера')}>
      <div className="settings-panel__heading">
        <div>
          <h2>{translate('Доступ к моделям')}</h2>
          <p className="shell__empty">{translate(schema.description)}</p>
        </div>
        <span className={`settings-panel__state settings-panel__state--${configured ? 'ready' : 'offline'}`}>
          {translate(badgeText)}
        </span>
      </div>

      {catalogStatus !== null ? <p className="provider-form__catalog-status" role="status">{translate(catalogStatus)}</p> : null}
      {freeEvidenceStatus !== null ? <p className="provider-form__catalog-status" role="status">{translate(freeEvidenceStatus)}</p> : null}

      <ProviderBaseFields
        schema={schema}
        selection={selection}
        choices={PROVIDER_CHOICES}
        onSelectChoice={(choice) => void selectChoice(choice)}
        apiKey={apiKey}
        onApiKeyChange={(value) => {
          setApiKey(value)
          setAcknowledgeProbePossibleCost(false)
        }}
        displayBaseUrl={displayBaseUrl}
        onBaseUrlChange={(value) => {
          setBaseUrl(value)
          setProbePolicy('disabled')
          setAcknowledgeProbePossibleCost(false)
        }}
        configured={configured}
        busy={busy}
      />

      <ProviderSpecificSettings
        schema={schema}
        busy={busy}
        connection={connection}
        events={events}
        baseUrl={baseUrl}
        accountId={accountId}
        onAccountIdChange={(value) => {
          setAccountId(value)
          setProbePolicy('disabled')
          setAcknowledgeProbePossibleCost(false)
        }}
        tier={tier}
        onTierChange={setTier}
        probePolicy={probePolicy}
        onProbePolicyChange={(policy) => {
          setProbePolicy(policy)
          setAcknowledgeProbePossibleCost(false)
        }}
        acknowledgeProbePossibleCost={acknowledgeProbePossibleCost}
        onAcknowledgeProbePossibleCostChange={setAcknowledgeProbePossibleCost}
        routingMode={routingMode}
        onRoutingModeChange={setRoutingMode}
        allowPaidFallback={allowPaidFallback}
        onAllowPaidFallbackChange={setAllowPaidFallback}
        pacingEnabled={pacingEnabled}
        onPacingEnabledChange={setPacingEnabled}
        pacingInterval={pacingInterval}
        onPacingIntervalChange={setPacingInterval}
        pacingFreeInterval={pacingFreeInterval}
        onPacingFreeIntervalChange={setPacingFreeInterval}
        pacingIsDefault={pacingIsDefault}
      />

      {schema.supportsModelLimits ? (
        <ModelSettingsBlock
          entries={modelLimitEntries}
          catalogModels={catalogModels}
          busy={busy}
          onUpdateEntry={updateModelLimitEntry}
          onAddEntry={addModelLimitEntry}
          onRemoveEntry={removeModelLimitEntry}
        />
      ) : null}

      <div className="provider-form__actions">
        <button type="button" onClick={() => void save()} disabled={!canSave}>
          {translate(schema.saveButtonLabel)}
        </button>
        {configured && schema.requiresApiKey ? (
          <button type="button" onClick={() => void clearKey()} disabled={busy}>
            {translate('Удалить ключ')}
          </button>
        ) : null}
      </div>

      {schema.supportsAutoProbe && configured ? (
        <div className="provider-form__catalog-status" aria-label={translate('Проверка бесплатного доступа')}>
          <p>{translate('Разовая проверка OpenRouter: Core сверит цену модели и выполнит один короткий synthetic completion. Возможен расход.')}</p>
          <button type="button" onClick={() => void verifyFreeAccess()} disabled={busy || model.trim().length === 0}>
            {translate('Проверить текущую модель')}
          </button>
          {probeStatus ? <p role="status">{translate(probeStatus)}</p> : null}
        </div>
      ) : null}

      {status.kind === 'saved' ? (
        <p className={status.restarted ? 'provider-form__ok' : 'shell__reason'}>
          {translate(status.restarted
            ? status.action === 'provider'
              ? 'Провайдер выбран и сохранён, Core перезапущен — подключение восстановится за пару секунд.'
              : 'Сохранено, Core перезапущен — подключение восстановится за пару секунд.'
            : status.action === 'provider'
              ? 'Провайдер сохранён, но Core не перезапустился. Перезапусти приложение вручную.'
              : 'Сохранено, но Core не перезапустился. Перезапусти приложение вручную.')}
        </p>
      ) : null}
      {status.kind === 'failed' ? (
        <p role="alert" className="shell__reason">{translate(status.message)}</p>
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
    ? formatDateTime(value['observed_at_ms'])
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
