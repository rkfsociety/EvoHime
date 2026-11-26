import { chmodSync, closeSync, fsyncSync, mkdirSync, openSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs'
import { randomUUID } from 'node:crypto'
import { dirname, join } from 'node:path'

import {
  PROVIDER_KINDS,
  PROVIDER_PROFILE_ENDPOINTS,
  PROVIDER_PROFILE_IDS,
  REQUEST_PACING_MAX_INTERVAL_MS,
  defaultRequestPacing,
  type ModelTier,
  type ProviderKind,
  type ProviderProfileId,
  type FreeAccessProbePolicy,
  type FreeAccessRoutingMode,
  type ProviderProfileSummary,
  type ProviderSummary,
  type RequestPacingSettings,
  type ProviderModelRateLimits,
  type ModelRateLimitSettings,
  OLLAMA_DEFAULT_BASE_URL
} from '@shared/api'

/**
 * Provider credentials owned by the main process.
 *
 * The renderer never sees the key: it sends one write-only update and reads
 * back a summary that only says whether a key is stored. The value itself is
 * encrypted by the OS (DPAPI on Windows via Electron `safeStorage`) and is
 * handed to Core the same way a developer launch does it — through the
 * environment of the supervisor that owns the Core process.
 */

export const MAX_KEY_CHARS = 512
export const MAX_MODEL_CHARS = 128
export const MAX_URL_CHARS = 512
const STORE_VERSION = 9
const MAX_STORED_SECRET_CHARS = 8_192
const MAX_REVOKED_CREDENTIAL_BINDINGS = 128

export interface ProviderUpdate {
  readonly provider: ProviderKind
  /** Empty string keeps the stored key; used when only the model changes. */
  readonly apiKey: string
  readonly model: string
  readonly baseUrl: string
  readonly tier: ModelTier
  readonly profileId?: ProviderProfileId
  readonly accountId?: string
  readonly freeAccessProbePolicy?: FreeAccessProbePolicy
  readonly acknowledgeProbePossibleCost?: boolean
  readonly freeAccessRoutingMode?: FreeAccessRoutingMode
  readonly allowPaidFallback?: boolean
  /** Omitted values keep the stored per-provider pacing settings. */
  readonly requestPacing?: RequestPacingSettings
  readonly modelRateLimits?: ProviderModelRateLimits
}

/** OS-backed encryption, injected so the store stays testable. */
export interface SecretCipher {
  isAvailable(): boolean
  encrypt(value: string): Buffer
  decrypt(value: Buffer): string
}

interface StoredProfile {
  readonly model: string
  readonly baseUrl: string
  readonly tier: ModelTier
  readonly secret: string
  readonly profileId: ProviderProfileId
  readonly accountId: string
  readonly credentialBinding: string
  readonly freeAccessProbePolicy: FreeAccessProbePolicy
  readonly requestPacing: RequestPacingSettings
  readonly modelRateLimits: ProviderModelRateLimits
}

interface StoredDocument {
  readonly provider: ProviderKind
  readonly profiles: Readonly<Partial<Record<ProviderKind, StoredProfile>>>
  readonly compatibleProfiles: Readonly<Partial<Record<ProviderProfileId, StoredProfile>>>
  readonly codexModel: string
  readonly hiveApiKeys: Readonly<Partial<Record<string, string>>>
  readonly hiveFallbackApiKeys: Readonly<Partial<Record<string, string>>>
  readonly freeAccessRoutingMode: FreeAccessRoutingMode
  readonly allowPaidFallback: boolean
  readonly revokedCredentialBindings: readonly string[]
}

const EMPTY: StoredDocument = {
  provider: 'literouter',
  profiles: {},
  compatibleProfiles: {},
  codexModel: '',
  hiveApiKeys: {},
  hiveFallbackApiKeys: {},
  freeAccessRoutingMode: 'any',
  allowPaidFallback: false,
  revokedCredentialBindings: []
}

export function isProviderKind(value: unknown): value is ProviderKind {
  return typeof value === 'string' && (PROVIDER_KINDS as readonly string[]).includes(value)
}

export function isProviderProfileId(value: unknown): value is ProviderProfileId {
  return typeof value === 'string' && (PROVIDER_PROFILE_IDS as readonly string[]).includes(value)
}

export function normalizeCloudflareAccountId(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const accountId = value.trim()
  return /^[a-fA-F0-9]{32}$/.test(accountId) ? accountId : null
}

/**
 * Accepts an https endpoint, or plain http only on the loopback interface.
 * The key is sent to this address, so an arbitrary http host would leak it.
 */
export function normalizeBaseUrl(value: unknown): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  if (trimmed.length === 0) {
    return ''
  }
  if (trimmed.length > MAX_URL_CHARS) {
    return null
  }
  let url: URL
  try {
    url = new URL(trimmed)
  } catch {
    return null
  }
  const loopback = url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '::1'
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) {
    return null
  }
  return url.toString().replace(/\/$/, '')
}

/** Model identifiers stay a bounded single-line token. */
export function normalizeModel(value: unknown): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  if (trimmed.length > MAX_MODEL_CHARS || /[\s\0]/.test(trimmed)) {
    return null
  }
  return trimmed
}

/** A key may not carry newlines: it is passed through an environment block. */
export function normalizeApiKey(value: unknown): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  if (trimmed.length > MAX_KEY_CHARS || /[\r\n\0]/.test(trimmed)) {
    return null
  }
  return trimmed
}

/** Validates one bounded pacing interval in milliseconds. */
function normalizePacingInterval(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isInteger(value)) return null
  return value >= 0 && value <= REQUEST_PACING_MAX_INTERVAL_MS ? value : null
}

/**
 * Validates per-provider request pacing. Values outside the accepted range
 * reject the whole write instead of silently changing the user's choice.
 */
export function normalizeRequestPacing(value: unknown): RequestPacingSettings | null {
  if (!isRecord(value) || typeof value['enabled'] !== 'boolean') return null
  const intervalMs = normalizePacingInterval(value['intervalMs'])
  const freeIntervalMs = normalizePacingInterval(value['freeIntervalMs'])
  if (intervalMs === null || freeIntervalMs === null) return null
  return { enabled: value['enabled'], intervalMs, freeIntervalMs }
}

const MAX_MODEL_RATE = 4_294_967_295
const MAX_MODEL_PRICE_USD_PER_1M = 10_000
const MAX_MODEL_RATE_LIMIT_MODELS = 64

function normalizeModelPricing(value: unknown): ModelRateLimitSettings['pricing'] | null | undefined {
  if (value === undefined) return undefined
  if (!isRecord(value)) return null
  const rawInput = value['inputUsdPer1M'] ?? value['input_usd_per_1m']
  const rawCached = value['cachedInputUsdPer1M'] ?? value['cached_input_usd_per_1m'] ?? 0
  const rawOutput = value['outputUsdPer1M'] ?? value['output_usd_per_1m']
  const rates = [rawInput, rawCached, rawOutput]
  for (const rate of rates) {
    if (typeof rate !== 'number' || !Number.isFinite(rate) || rate < 0 || rate > MAX_MODEL_PRICE_USD_PER_1M) {
      return null
    }
  }
  const peakInput = value['peakInputUsdPer1M'] ?? value['peak_input_usd_per_1m']
  const peakCached = value['peakCachedInputUsdPer1M'] ?? value['peak_cached_input_usd_per_1m']
  const peakOutput = value['peakOutputUsdPer1M'] ?? value['peak_output_usd_per_1m']
  for (const rate of [peakInput, peakCached, peakOutput]) {
    if (rate !== undefined && (typeof rate !== 'number' || !Number.isFinite(rate) || rate < 0 || rate > MAX_MODEL_PRICE_USD_PER_1M)) {
      return null
    }
  }
  return {
    inputUsdPer1M: rawInput as number,
    cachedInputUsdPer1M: rawCached as number,
    outputUsdPer1M: rawOutput as number,
    ...(peakInput !== undefined ? { peakInputUsdPer1M: peakInput as number } : {}),
    ...(peakCached !== undefined ? { peakCachedInputUsdPer1M: peakCached as number } : {}),
    ...(peakOutput !== undefined ? { peakOutputUsdPer1M: peakOutput as number } : {})
  }
}

function normalizeModelCapabilities(value: unknown): ModelRateLimitSettings['capabilities'] | null | undefined {
  if (value === undefined) return undefined
  if (!isRecord(value)) return null
  const keys = [
    'jsonOutput',
    'toolCalls',
    'responsesApi',
    'anthropicApi',
    'chatPrefixCompletion',
    'fimCompletion',
    'vision'
  ] as const
  const result: Record<string, boolean> = {}
  for (const key of keys) {
    const flag = value[key]
    if (flag !== undefined) {
      if (typeof flag !== 'boolean') return null
      result[key] = flag
    }
  }
  return result
}

function normalizeModelRate(value: unknown): ModelRateLimitSettings | null {
  if (!isRecord(value) || typeof value['enabled'] !== 'boolean') return null
  const numbers = ['rpm', 'rph', 'rpd'] as const
  const result = { enabled: value['enabled'] } as {
    enabled: boolean
    rpm: number
    rph: number
    rp4h?: number
    rpd: number
  }
  for (const key of numbers) {
    const n = value[key]
    if (typeof n !== 'number' || !Number.isInteger(n) || n < 0 || n > MAX_MODEL_RATE) return null
    result[key] = n
  }
  const rp4h = value['rp4h']
  if (rp4h !== undefined) {
    if (typeof rp4h !== 'number' || !Number.isInteger(rp4h) || rp4h < 0 || rp4h > MAX_MODEL_RATE) return null
    result.rp4h = rp4h
  }
  const pricing = normalizeModelPricing(value['pricing'])
  if (pricing === null) return null
  const capabilities = normalizeModelCapabilities(value['capabilities'])
  if (capabilities === null) return null
  const concurrencyLimit = value['concurrencyLimit'] ?? value['concurrency_limit']
  if (concurrencyLimit !== undefined && (typeof concurrencyLimit !== 'number' || !Number.isInteger(concurrencyLimit) || concurrencyLimit < 0 || concurrencyLimit > MAX_MODEL_RATE)) return null
  const contextLengthTokens = value['contextLengthTokens'] ?? value['context_length_tokens']
  if (contextLengthTokens !== undefined && (typeof contextLengthTokens !== 'number' || !Number.isInteger(contextLengthTokens) || contextLengthTokens < 0 || contextLengthTokens > 1_000_000_000)) return null
  const contextCompressionTokens = value['contextCompressionTokens'] ?? value['context_compression_tokens']
  if (contextCompressionTokens !== undefined && (typeof contextCompressionTokens !== 'number' || !Number.isInteger(contextCompressionTokens) || contextCompressionTokens < 0 || contextCompressionTokens > 1_000_000_000)) return null
  const maxOutputTokens = value['maxOutputTokens'] ?? value['max_output_tokens']
  if (maxOutputTokens !== undefined && (typeof maxOutputTokens !== 'number' || !Number.isInteger(maxOutputTokens) || maxOutputTokens < 0 || maxOutputTokens > 1_000_000_000)) return null
  const dailyTokenQuota = value['dailyTokenQuota'] ?? value['daily_token_quota']
  if (dailyTokenQuota !== undefined && dailyTokenQuota !== null &&
    (typeof dailyTokenQuota !== 'number' || !Number.isSafeInteger(dailyTokenQuota) || dailyTokenQuota < 0 || dailyTokenQuota > 1_000_000_000_000)) return null
  const quotaResetUtcMidnight = value['quotaResetUtcMidnight'] ?? value['quota_reset_utc_midnight']
  if (quotaResetUtcMidnight !== undefined && typeof quotaResetUtcMidnight !== 'boolean') return null
  const resetScheduleEnabled = value['resetScheduleEnabled'] ?? value['reset_schedule_enabled']
  if (resetScheduleEnabled !== undefined && typeof resetScheduleEnabled !== 'boolean') return null
  const resetTimeMinutes = value['resetTimeMinutes'] ?? value['reset_time_minutes']
  if (
    resetTimeMinutes !== undefined
    && (typeof resetTimeMinutes !== 'number' || !Number.isInteger(resetTimeMinutes) || resetTimeMinutes < 0 || resetTimeMinutes >= 1_440)
  ) return null
  const resetUtcOffsetHours = value['resetUtcOffsetHours'] ?? value['reset_utc_offset_hours']
  if (
    resetUtcOffsetHours !== undefined
    && (typeof resetUtcOffsetHours !== 'number' || !Number.isInteger(resetUtcOffsetHours) || resetUtcOffsetHours < -12 || resetUtcOffsetHours > 14)
  ) return null
  return {
    ...result,
    ...(concurrencyLimit !== undefined ? { concurrencyLimit } : {}),
    ...(contextLengthTokens !== undefined ? { contextLengthTokens } : {}),
    ...(contextCompressionTokens !== undefined ? { contextCompressionTokens } : {}),
    ...(maxOutputTokens !== undefined ? { maxOutputTokens } : {}),
    ...(capabilities !== undefined ? { capabilities } : {}),
    ...(pricing !== undefined ? { pricing } : {}),
    ...(dailyTokenQuota !== undefined ? { dailyTokenQuota: dailyTokenQuota as number | null } : {}),
    ...(quotaResetUtcMidnight !== undefined ? { quotaResetUtcMidnight } : {}),
    ...(resetScheduleEnabled !== undefined ? { resetScheduleEnabled } : {}),
    ...(resetTimeMinutes !== undefined ? { resetTimeMinutes } : {}),
    ...(resetUtcOffsetHours !== undefined ? { resetUtcOffsetHours } : {})
  }
}
export function normalizeModelRateLimits(value: unknown): ProviderModelRateLimits | null {
  if (value === undefined) return {}
  if (!isRecord(value) || Object.keys(value).length > MAX_MODEL_RATE_LIMIT_MODELS) return null
  const result: Record<string, ModelRateLimitSettings> = {}
  for (const [model, settings] of Object.entries(value)) {
    if ((model.length === 0 && model !== '*') || model.length > MAX_MODEL_CHARS || /[\s\0]/.test(model)) return null
    const normalized = normalizeModelRate(settings)
    if (normalized === null) return null
    result[model] = normalized
  }
  return result
}

export class ProviderStore {
  constructor(
    private readonly filePath: string,
    private readonly cipher: SecretCipher
  ) {}

  static defaultPath(dataDirectory: string): string {
    return join(dataDirectory, 'shell', 'provider.json')
  }

  /** Secret-free summary for the settings surface. */
  summary(): ProviderSummary {
    const document = this.readDocument()
    return this.summaryFor(document)
  }

  private summaryFor(document: StoredDocument): ProviderSummary {
    const active = profileFor(document, document.provider)
    const hiveApiKeysConfigured = configuredHiveKeysMap(document.hiveApiKeys)
    const hiveFallbackApiKeysConfigured = configuredHiveKeysMap(document.hiveFallbackApiKeys)
    return {
      provider: document.provider,
      model: active.model,
      baseUrl: active.baseUrl,
      tier: active.tier,
      configured: configuredProfile(document.provider, active),
      profileId: active.profileId,
      ...(active.accountId ? { accountId: active.accountId } : {}),
      freeAccessRoutingMode: document.freeAccessRoutingMode,
      allowPaidFallback: document.allowPaidFallback,
      hiveApiKeyConfigured: Object.keys(hiveApiKeysConfigured).length > 0,
      hiveFallbackApiKeyConfigured: Object.keys(hiveFallbackApiKeysConfigured).length > 0,
      hiveApiKeysConfigured,
      hiveFallbackApiKeysConfigured,
      requestPacing: active.requestPacing,
      modelRateLimits: active.modelRateLimits,
      profiles: Object.fromEntries(PROVIDER_KINDS.map((kind) => (
        [kind, profileSummary(kind, profileFor(document, kind))] as const
      ))) as Readonly<Record<ProviderKind, ProviderProfileSummary>>,
      // Unsaved vendor profiles stay absent so the renderer can tell a stored
      // credential apart from its defaults.
      compatibleProfiles: Object.fromEntries(PROVIDER_PROFILE_IDS.flatMap((profileId) => {
        const profile = document.compatibleProfiles[profileId]
        return profile ? [[profileId, profileSummary('openai_compatible', profile)] as const] : []
      })) as Partial<Record<ProviderProfileId, ProviderProfileSummary>>
    }
  }

  /**
   * Persists the update. Returns null when the OS refuses to encrypt: the key
   * is dropped rather than written in the clear.
   */
  save(update: ProviderUpdate): ProviderSummary | null {
    const current = this.readDocument()
    const requestedKey = normalizeApiKey(update.apiKey)
    if (requestedKey === null) return null
    const profileId = update.provider === 'openai_compatible'
      ? update.profileId ?? inferProviderProfileId(update.baseUrl)
      : 'custom'
    const previous = update.provider === 'openai_compatible'
      ? compatibleProfileFor(current, profileId)
      : profileFor(current, update.provider)
    let secret = previous.secret
    if (requestedKey.length > 0) {
      if (!this.cipher.isAvailable()) {
        return null
      }
      secret = this.cipher.encrypt(requestedKey).toString('base64')
    }
    const accountId = profileId === 'cloudflare_workers_ai'
      ? normalizeCloudflareAccountId(update.accountId) ?? cloudflareAccountFromBaseUrl(update.baseUrl) ?? ''
      : ''
    const baseUrl = update.provider === 'openai_compatible'
      ? providerProfileBaseUrl(profileId, accountId, update.baseUrl)
      : normalizeBaseUrl(update.baseUrl)
    if (baseUrl === null || (profileId === 'cloudflare_workers_ai' && accountId.length === 0)) {
      return null
    }
    const previousKey = this.decryptSecret(previous.secret)
    const hasCredential = secret.length > 0 && this.decryptSecret(secret).length > 0
    const credentialBinding = hasCredential
      ? (requestedKey.length > 0 && requestedKey !== previousKey
          ? newCredentialBinding()
          : normalizeCredentialBinding(previous.credentialBinding) ?? newCredentialBinding())
      : ''
    const credentialChanged = credentialBinding !== previous.credentialBinding
    const profileChanged = profileId !== previous.profileId
      || accountId !== previous.accountId
      || baseUrl !== previous.baseUrl
    const requestedProbePolicy = isFreeAccessProbePolicy(update.freeAccessProbePolicy)
      ? update.freeAccessProbePolicy
      : previous.freeAccessProbePolicy
    const policyChanged = requestedProbePolicy !== previous.freeAccessProbePolicy
    const consentScopeChanged = credentialChanged || profileChanged
    const automaticPolicy = requestedProbePolicy === 'on_first_use' || requestedProbePolicy === 'periodic_bounded'
    if (automaticPolicy && (policyChanged || consentScopeChanged) && update.acknowledgeProbePossibleCost !== true) {
      return null
    }
    const freeAccessProbePolicy = consentScopeChanged && update.acknowledgeProbePossibleCost !== true
      ? 'disabled'
      : requestedProbePolicy
    const requestPacing = update.requestPacing === undefined
      ? previous.requestPacing
      : normalizeRequestPacing(update.requestPacing)
    if (requestPacing === null) return null
    const modelRateLimits = update.modelRateLimits === undefined
      ? previous.modelRateLimits
      : normalizeModelRateLimits(update.modelRateLimits)
    if (modelRateLimits === null) return null
    const revokedCredentialBindings = credentialChanged && previous.credentialBinding
      ? appendRevokedBinding(current.revokedCredentialBindings, previous.credentialBinding)
      : current.revokedCredentialBindings
    const next: StoredDocument = {
      provider: update.provider,
      profiles: {
        ...current.profiles,
        [update.provider]: {
          model: update.model,
          baseUrl,
          tier: update.tier,
          secret,
          profileId,
          accountId,
          credentialBinding,
          freeAccessProbePolicy,
          requestPacing,
          modelRateLimits
        }
      },
      compatibleProfiles: update.provider === 'openai_compatible'
        ? {
            ...current.compatibleProfiles,
            [profileId]: {
              model: update.model,
              baseUrl,
              tier: update.tier,
              secret,
              profileId,
              accountId,
              credentialBinding,
              freeAccessProbePolicy,
              requestPacing,
              modelRateLimits
            }
          }
        : current.compatibleProfiles,
      codexModel: current.codexModel,
      hiveApiKeys: current.hiveApiKeys,
      hiveFallbackApiKeys: current.hiveFallbackApiKeys,
      freeAccessRoutingMode: isFreeAccessRoutingMode(update.freeAccessRoutingMode)
        ? update.freeAccessRoutingMode
        : current.freeAccessRoutingMode,
      allowPaidFallback: update.allowPaidFallback === undefined
        ? current.allowPaidFallback
        : update.allowPaidFallback === true,
      revokedCredentialBindings
    }
    this.write(next)
    return this.summaryFor(next)
  }

  /** Changes the active API profile without touching any stored credential. */
  select(provider: ProviderKind, profileId?: ProviderProfileId): ProviderSummary {
    const current = this.readDocument()
    const selectedProfileId = provider === 'openai_compatible'
      ? profileId ?? (current.provider === 'openai_compatible' ? profileFor(current, provider).profileId : 'openai')
      : undefined
    if (current.provider === provider && (provider !== 'openai_compatible' || (
      selectedProfileId !== undefined && current.profiles.openai_compatible?.profileId === selectedProfileId &&
      current.compatibleProfiles[selectedProfileId] !== undefined
    ))) {
      return this.summaryFor(current)
    }
    const next = provider === 'openai_compatible' && selectedProfileId
      ? {
          ...current,
          provider,
          profiles: { ...current.profiles, openai_compatible: compatibleProfileFor(current, selectedProfileId) },
          compatibleProfiles: {
            ...current.compatibleProfiles,
            [selectedProfileId]: compatibleProfileFor(current, selectedProfileId)
          }
        }
      : { ...current, provider }
    this.write(next)
    return this.summaryFor(next)
  }

  /** Forgets the stored key while keeping the provider choice. */
  clearKey(provider = this.readDocument().provider, profileId?: ProviderProfileId): ProviderSummary {
    const current = this.readDocument()
    const selectedProfileId = provider === 'openai_compatible'
      ? profileId ?? (current.provider === 'openai_compatible' ? profileFor(current, provider).profileId : 'openai')
      : undefined
    const active = selectedProfileId
      ? compatibleProfileFor(current, selectedProfileId)
      : profileFor(current, provider)
    const cleared = { ...active, secret: '', credentialBinding: '', freeAccessProbePolicy: 'disabled' as const }
    const next: StoredDocument = {
      ...current,
      provider,
      profiles: selectedProfileId && current.profiles.openai_compatible?.profileId !== selectedProfileId
        ? current.profiles
        : { ...current.profiles, [provider]: cleared },
      compatibleProfiles: selectedProfileId
        ? { ...current.compatibleProfiles, [selectedProfileId]: cleared }
        : current.compatibleProfiles,
      revokedCredentialBindings: active.credentialBinding
        ? appendRevokedBinding(current.revokedCredentialBindings, active.credentialBinding)
        : current.revokedCredentialBindings
    }
    this.write(next)
    return this.summaryFor(next)
  }

  codexModel(): string {
    return this.readDocument().codexModel
  }

  /**
   * Persists the model chosen for the active provider without touching the
   * credential. Core gets it back through {@link environment} on the next
   * restart, so a selected model survives the application restart instead of
   * falling back to the first catalogue entry.
   */
  saveSelectedModel(model: string): void {
    const normalized = normalizeModel(model)
    if (normalized === null || normalized.length === 0) return
    const current = this.readDocument()
    const profile = profileFor(current, current.provider)
    if (normalized === profile.model) return
    const next: StoredDocument = {
      ...current,
      profiles: {
        ...current.profiles,
        [current.provider]: { ...profile, model: normalized }
      }
    }
    if (current.provider === 'openai_compatible') {
      const compatibleProfiles = {
        ...current.compatibleProfiles,
        [profile.profileId]: { ...profile, model: normalized }
      }
      this.write({ ...next, compatibleProfiles })
      return
    }
    this.write(next)
  }

  saveCodexModel(model: string): void {
    const current = this.readDocument()
    this.write({ ...current, codexModel: model })
  }

  /**
   * Persists or clears the isolated API key for a specific Hive provider encrypted via OS cipher.
   * Returns false when the provider/key is invalid or OS encryption is unavailable.
   */
  saveHiveApiKey(apiKey: string, provider?: string): boolean {
    const providerKey = normalizeHiveProviderKey(provider)
    if (providerKey === null) return false
    const normalized = normalizeApiKey(apiKey)
    if (normalized === null) return false
    const current = this.readDocument()
    const nextKeys = { ...current.hiveApiKeys }
    if (normalized.length === 0) {
      if (!(providerKey in nextKeys)) return true
      delete nextKeys[providerKey]
      this.write({ ...current, hiveApiKeys: nextKeys })
      return true
    }
    if (!this.cipher.isAvailable()) {
      return false
    }
    nextKeys[providerKey] = this.cipher.encrypt(normalized).toString('base64')
    this.write({ ...current, hiveApiKeys: nextKeys })
    return true
  }

  /**
   * Persists or clears the isolated fallback API key for a specific Hive provider encrypted via OS cipher.
   * Returns false when the provider/key is invalid or OS encryption is unavailable.
   */
  saveHiveFallbackApiKey(apiKey: string, provider?: string): boolean {
    const providerKey = normalizeHiveProviderKey(provider)
    if (providerKey === null) return false
    const normalized = normalizeApiKey(apiKey)
    if (normalized === null) return false
    const current = this.readDocument()
    const nextKeys = { ...current.hiveFallbackApiKeys }
    if (normalized.length === 0) {
      if (!(providerKey in nextKeys)) return true
      delete nextKeys[providerKey]
      this.write({ ...current, hiveFallbackApiKeys: nextKeys })
      return true
    }
    if (!this.cipher.isAvailable()) {
      return false
    }
    nextKeys[providerKey] = this.cipher.encrypt(normalized).toString('base64')
    this.write({ ...current, hiveFallbackApiKeys: nextKeys })
    return true
  }

  /**
   * Environment block for the Core process. Only the variables of the selected
   * provider are set, so a stale key of the other one never reaches the model
   * gateway.
   */
  environment(): Record<string, string> {
    const document = this.readDocument()
    const profile = profileFor(document, document.provider)
    const key = this.decryptSecret(profile.secret)
    const environment: Record<string, string> = {
      MODEL_PROVIDER: document.provider,
      MODEL_FREE_ACCESS_ROUTING_MODE: document.freeAccessRoutingMode,
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: String(document.allowPaidFallback),
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: profile.freeAccessProbePolicy,
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: document.revokedCredentialBindings.join(','),
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: String(profile.requestPacing.enabled),
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: String(profile.requestPacing.intervalMs),
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: String(profile.requestPacing.freeIntervalMs)
    }
    environment['MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON'] = JSON.stringify(profile.modelRateLimits)
    if (document.codexModel) environment['CODEX_MODEL'] = document.codexModel
    const decryptedHiveKeys = this.decryptHiveKeys(document.hiveApiKeys)
    if (Object.keys(decryptedHiveKeys).length > 0) {
      environment['HIVE_API_KEYS_JSON'] = JSON.stringify(decryptedHiveKeys)
      if (decryptedHiveKeys['rout']) {
        environment['HIVE_API_KEY'] = decryptedHiveKeys['rout']
      }
    }
    const decryptedFallbackKeys = this.decryptHiveKeys(document.hiveFallbackApiKeys)
    if (Object.keys(decryptedFallbackKeys).length > 0) {
      environment['HIVE_FALLBACK_API_KEYS_JSON'] = JSON.stringify(decryptedFallbackKeys)
      if (decryptedFallbackKeys['rout']) {
        environment['HIVE_FALLBACK_API_KEY'] = decryptedFallbackKeys['rout']
      }
    }
    const additionalRoutes = this.collectAdditionalRoutes(document)
    if (Object.keys(additionalRoutes).length > 0) {
      environment['MODEL_ADDITIONAL_ROUTES_JSON'] = JSON.stringify(additionalRoutes)
    }
    if (document.provider === 'ollama') {
      environment['OLLAMA_BASE_URL'] = profile.baseUrl || OLLAMA_DEFAULT_BASE_URL
      if (profile.model) environment['OLLAMA_MODEL'] = profile.model
      return environment
    }
    if (document.provider === 'openai_compatible' || document.provider === 'openai_responses') {
      if (key) environment['OPENAI_API_KEY'] = key
      if (profile.baseUrl) environment['OPENAI_BASE_URL'] = profile.baseUrl
      if (profile.model) environment['OPENAI_MODEL'] = profile.model
      if (document.provider === 'openai_compatible') {
        environment['MODEL_PROVIDER_PROFILE_ID'] = profile.profileId
        if (profile.profileId === 'cloudflare_workers_ai' && profile.accountId) {
          environment['MODEL_PROVIDER_ACCOUNT_ID'] = profile.accountId
        }
      }
      if (key && profile.credentialBinding) {
        environment['MODEL_PROVIDER_CREDENTIAL_BINDING'] = profile.credentialBinding
        if (profile.freeAccessProbePolicy === 'passive_only' || profile.freeAccessProbePolicy === 'on_first_use' || profile.freeAccessProbePolicy === 'periodic_bounded') {
          environment['MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING'] = profile.credentialBinding
        }
      }
      return environment
    }
    if (key && profile.credentialBinding) {
      environment['MODEL_PROVIDER_CREDENTIAL_BINDING'] = profile.credentialBinding
      if (profile.freeAccessProbePolicy === 'passive_only' || profile.freeAccessProbePolicy === 'on_first_use' || profile.freeAccessProbePolicy === 'periodic_bounded') {
        environment['MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING'] = profile.credentialBinding
      }
    }
    if (key) environment['LITEROUTER_API_KEY'] = key
    if (profile.baseUrl) environment['LITEROUTER_BASE_URL'] = profile.baseUrl
    if (profile.model) environment['LITEROUTER_MODEL'] = profile.model
    const routProfile = document.compatibleProfiles['rout']
    if (routProfile) {
      const routKey = this.decryptSecret(routProfile.secret)
      if (routKey) {
        environment['ROUT_API_KEY'] = routKey
        environment['ROUT_BASE_URL'] = routProfile.baseUrl || PROVIDER_PROFILE_ENDPOINTS.rout || 'https://api.rout.my/v1'
        if (routProfile.model) environment['ROUT_MODEL'] = routProfile.model
      }
    }
    return environment
  }

  private collectAdditionalRoutes(document: StoredDocument): Record<string, Record<string, unknown>> {
    const routes: Record<string, Record<string, unknown>> = {}
    const activeCompatibleProfileId =
      document.provider === 'openai_compatible'
        ? profileFor(document, 'openai_compatible').profileId
        : undefined

    const addProfileRoute = (
      routeKey: string,
      kind: ProviderKind,
      stored: StoredProfile,
      apiKey: string
    ): void => {
      routes[routeKey] = {
        provider: kind,
        ...(apiKey ? { api_key: apiKey } : {}),
        ...(stored.baseUrl ? { base_url: stored.baseUrl } : {}),
        ...(stored.model ? { model: stored.model } : {}),
        ...(kind === 'openai_compatible' ? { provider_profile_id: stored.profileId } : {}),
        ...(kind === 'openai_compatible' && stored.profileId === 'cloudflare_workers_ai' && stored.accountId
          ? { provider_account_id: stored.accountId }
          : {}),
        ...(stored.credentialBinding ? { provider_credential_binding: stored.credentialBinding } : {}),
        model_rate_limits: stored.modelRateLimits
      }
    }

    for (const kind of PROVIDER_KINDS) {
      if (kind === document.provider) continue
      const stored = document.profiles[kind]
      if (!stored) continue
      if (kind === 'ollama') {
        if (stored.baseUrl || stored.model) {
          addProfileRoute('ollama', 'ollama', stored, '')
        }
        continue
      }
      const apiKey = this.decryptSecret(stored.secret)
      if (!apiKey) continue
      addProfileRoute(kind, kind, stored, apiKey)
    }

    for (const profileId of PROVIDER_PROFILE_IDS) {
      if (profileId === activeCompatibleProfileId) continue
      const stored = document.compatibleProfiles[profileId]
      if (!stored) continue
      const apiKey = this.decryptSecret(stored.secret)
      if (!apiKey) continue
      addProfileRoute(`openai_compatible:${profileId}`, 'openai_compatible', stored, apiKey)
    }

    return routes
  }

  private decryptHiveKeys(secrets: Readonly<Partial<Record<string, string>>>): Record<string, string> {
    const result: Record<string, string> = {}
    for (const [provider, secret] of Object.entries(secrets)) {
      if (typeof secret !== 'string' || secret.length === 0) continue
      const key = this.decryptSecret(secret)
      if (key) {
        result[provider] = key
      }
    }
    return result
  }

  private decryptSecret(secret: string): string {
    if (secret.length === 0 || !this.cipher.isAvailable()) {
      return ''
    }
    try {
      return this.cipher.decrypt(Buffer.from(secret, 'base64'))
    } catch {
      // A key encrypted for another user or a corrupt file must not take the
      // shell down; the settings surface simply reports it as not configured.
      return ''
    }
  }

  private readDocument(): StoredDocument {
    let raw: string
    try {
      raw = readFileSync(this.filePath, 'utf8')
    } catch {
      return EMPTY
    }
    let parsed: unknown
    try {
      parsed = JSON.parse(raw)
    } catch {
      return EMPTY
    }
    if (typeof parsed !== 'object' || parsed === null) {
      return EMPTY
    }
    const record = parsed as Record<string, unknown>
    const provider = isProviderKind(record['provider']) ? record['provider'] : EMPTY.provider
    const profiles: Partial<Record<ProviderKind, StoredProfile>> = {}
    const compatibleProfiles: Partial<Record<ProviderProfileId, StoredProfile>> = {}
    let needsMigration = record['version'] !== STORE_VERSION
    let hasCurrentProfiles = false
    if (isRecord(record['profiles'])) {
      hasCurrentProfiles = true
      for (const kind of PROVIDER_KINDS) {
        const value = record['profiles'][kind]
        if (!isRecord(value)) continue
        const baseUrl = normalizeBaseUrl(value['baseUrl']) ?? ''
        const isLegacyRoutMy = kind === 'openai_compatible' && value['profileId'] === 'routmy'
        if (isLegacyRoutMy) needsMigration = true
        const profileId = kind === 'openai_compatible'
          ? (isLegacyRoutMy
              ? 'rout'
              : (isProviderProfileId(value['profileId']) ? value['profileId'] : inferProviderProfileId(baseUrl)))
          : 'custom'
        const accountId = profileId === 'cloudflare_workers_ai'
          ? normalizeCloudflareAccountId(value['accountId']) ?? cloudflareAccountFromBaseUrl(baseUrl) ?? ''
          : ''
        const secret = normalizeStoredSecret(value['secret'])
        const existingBinding = normalizeCredentialBinding(value['credentialBinding'])
        const credentialBinding = secret.length > 0
          ? existingBinding ?? newCredentialBinding()
          : ''
        if (credentialBinding !== existingBinding) needsMigration = true
        const freeAccessProbePolicy = isFreeAccessProbePolicy(value['freeAccessProbePolicy'])
          ? value['freeAccessProbePolicy']
          : 'disabled'
        if (freeAccessProbePolicy !== value['freeAccessProbePolicy']) needsMigration = true
        const storedPacing = normalizeRequestPacing(value['requestPacing'])
        const modelRateLimits = normalizeModelRateLimits(value['modelRateLimits'])
        if (storedPacing === null || modelRateLimits === null || value['modelRateLimits'] === undefined) needsMigration = true
        const model = normalizeModel(value['model']) ?? ''
        profiles[kind] = {
          model,
          baseUrl: providerProfileBaseUrl(profileId, accountId, baseUrl) ?? '',
          tier: resolveProfileTier(value['tier'], kind, profileId, model),
          secret,
          profileId,
          accountId,
          credentialBinding,
          freeAccessProbePolicy,
          requestPacing: storedPacing ?? defaultRequestPacing(kind),
          modelRateLimits: modelRateLimits ?? {}
        }
      }
    } else {
      // Version 1 stored one active profile. Preserve it under that provider.
      const secret = normalizeStoredSecret(record['secret'])
      const existingBinding = normalizeCredentialBinding(record['credentialBinding'])
      const credentialBinding = secret.length > 0
        ? existingBinding ?? newCredentialBinding()
        : ''
      if (credentialBinding !== existingBinding) needsMigration = true
      const freeAccessProbePolicy = isFreeAccessProbePolicy(record['freeAccessProbePolicy'])
        ? record['freeAccessProbePolicy']
        : 'disabled'
      if (freeAccessProbePolicy !== record['freeAccessProbePolicy']) needsMigration = true
      const modelRateLimits = normalizeModelRateLimits(record['modelRateLimits'])
      if (modelRateLimits === null || record['modelRateLimits'] === undefined) needsMigration = true
      const legacyModel = normalizeModel(record['model']) ?? ''
      const legacyBaseUrl = normalizeBaseUrl(record['baseUrl']) ?? ''
      const legacyProfileId = provider === 'openai_compatible'
        ? inferProviderProfileId(legacyBaseUrl)
        : 'custom'
      profiles[provider] = {
        model: legacyModel,
        baseUrl: legacyBaseUrl,
        tier: resolveProfileTier(record['tier'], provider, legacyProfileId, legacyModel),
        secret,
        profileId: legacyProfileId,
        accountId: cloudflareAccountFromBaseUrl(normalizeBaseUrl(record['baseUrl']) ?? '') ?? '',
        credentialBinding,
        freeAccessProbePolicy,
        requestPacing: defaultRequestPacing(provider),
        modelRateLimits: modelRateLimits ?? {}
      }
      if (provider === 'openai_compatible') {
        const migrated = profiles[provider]
        if (migrated) {
          profiles[provider] = {
            ...migrated,
            baseUrl: providerProfileBaseUrl(migrated.profileId, migrated.accountId, migrated.baseUrl) ?? ''
          }
        }
      }
    }
    if (isRecord(record['compatibleProfiles'])) {
      for (const profileId of PROVIDER_PROFILE_IDS) {
        const value = record['compatibleProfiles'][profileId]
        if (!isRecord(value)) continue
        const rawBaseUrl = normalizeBaseUrl(value['baseUrl']) ?? ''
        const accountId = profileId === 'cloudflare_workers_ai'
          ? normalizeCloudflareAccountId(value['accountId']) ?? cloudflareAccountFromBaseUrl(rawBaseUrl) ?? ''
          : ''
        const baseUrl = providerProfileBaseUrl(profileId, accountId, rawBaseUrl) ?? ''
        const secret = normalizeStoredSecret(value['secret'])
        const existingBinding = normalizeCredentialBinding(value['credentialBinding'])
        const credentialBinding = secret.length > 0 ? existingBinding ?? newCredentialBinding() : ''
        const freeAccessProbePolicy = isFreeAccessProbePolicy(value['freeAccessProbePolicy'])
          ? value['freeAccessProbePolicy']
          : 'disabled'
        const storedPacing = normalizeRequestPacing(value['requestPacing'])
        const modelRateLimits = normalizeModelRateLimits(value['modelRateLimits'])
        if (credentialBinding !== existingBinding || freeAccessProbePolicy !== value['freeAccessProbePolicy'] ||
          storedPacing === null || modelRateLimits === null || value['modelRateLimits'] === undefined || baseUrl !== value['baseUrl']) needsMigration = true
        const compatModel = normalizeModel(value['model']) ?? ''
        compatibleProfiles[profileId] = {
          model: compatModel,
          baseUrl,
          tier: resolveProfileTier(value['tier'], 'openai_compatible', profileId, compatModel),
          secret,
          profileId,
          accountId,
          credentialBinding,
          freeAccessProbePolicy,
          requestPacing: storedPacing ?? defaultRequestPacing('openai_compatible'),
          modelRateLimits: modelRateLimits ?? {}
        }
      }
      const legacyRoutMy = record['compatibleProfiles']['routmy']
      if (isRecord(legacyRoutMy)) {
        needsMigration = true
        const rawBaseUrl = normalizeBaseUrl(legacyRoutMy['baseUrl']) ?? ''
        const baseUrl = providerProfileBaseUrl('rout', '', rawBaseUrl) ?? ''
        const secret = normalizeStoredSecret(legacyRoutMy['secret'])
        const existingBinding = normalizeCredentialBinding(legacyRoutMy['credentialBinding'])
        const credentialBinding = secret.length > 0 ? existingBinding ?? newCredentialBinding() : ''
        const freeAccessProbePolicy = isFreeAccessProbePolicy(legacyRoutMy['freeAccessProbePolicy'])
          ? legacyRoutMy['freeAccessProbePolicy']
          : 'disabled'
        const storedPacing = normalizeRequestPacing(legacyRoutMy['requestPacing'])
        const modelRateLimits = normalizeModelRateLimits(legacyRoutMy['modelRateLimits'])
        const compatModel = normalizeModel(legacyRoutMy['model']) ?? ''
        const existingRout = compatibleProfiles['rout']
        if (!existingRout) {
          compatibleProfiles['rout'] = {
            model: compatModel,
            baseUrl,
            tier: resolveProfileTier(legacyRoutMy['tier'], 'openai_compatible', 'rout', compatModel),
            secret,
            profileId: 'rout',
            accountId: '',
            credentialBinding,
            freeAccessProbePolicy,
            requestPacing: storedPacing ?? defaultRequestPacing('openai_compatible'),
            modelRateLimits: modelRateLimits ?? {}
          }
        } else if (existingRout.secret.length === 0 && secret.length > 0) {
          compatibleProfiles['rout'] = {
            ...existingRout,
            secret,
            credentialBinding,
            model: existingRout.model || compatModel,
            baseUrl: existingRout.baseUrl || baseUrl
          }
        }
      }
    }
    const legacyCompatible = profiles.openai_compatible
    if (legacyCompatible) {
      if (!compatibleProfiles[legacyCompatible.profileId]) {
        compatibleProfiles[legacyCompatible.profileId] = legacyCompatible
        needsMigration = true
      } else if (hasCurrentProfiles && record['version'] === STORE_VERSION) {
        // Version 8 writes the active compatibility profile in both places for
        // legacy readers. Do not let this mirror overwrite newer profile data.
        const active = compatibleProfiles[legacyCompatible.profileId]
        if (active && !sameStoredProfile(active, legacyCompatible)) {
          profiles.openai_compatible = active
          needsMigration = true
        }
      }
    }
    const codexModel = normalizeModel(record['codexModel']) ?? ''
    const hiveApiKeys = normalizeStoredHiveKeys(record['hiveApiKeys'])
    const hiveFallbackApiKeys = normalizeStoredHiveKeys(record['hiveFallbackApiKeys'])
    if (
      normalizeStoredSecret(record['hiveApiKeySecret']).length > 0 ||
      normalizeStoredSecret(record['hiveFallbackApiKeySecret']).length > 0
    ) {
      needsMigration = true
    }
    const freeAccessRoutingMode = isFreeAccessRoutingMode(record['freeAccessRoutingMode'])
      ? record['freeAccessRoutingMode']
      : 'any'
    const allowPaidFallback = record['allowPaidFallback'] === true
    const revokedCredentialBindings = Array.isArray(record['revokedCredentialBindings'])
      ? [...new Set(record['revokedCredentialBindings']
        .map(normalizeCredentialBinding)
        .filter((binding): binding is string => binding !== null))]
        .slice(-MAX_REVOKED_CREDENTIAL_BINDINGS)
      : []
    if (freeAccessRoutingMode !== record['freeAccessRoutingMode'] || allowPaidFallback !== record['allowPaidFallback'] ||
      JSON.stringify(revokedCredentialBindings) !== JSON.stringify(record['revokedCredentialBindings'] ?? [])) {
      needsMigration = true
    }
    const document = { provider, profiles, compatibleProfiles, codexModel, hiveApiKeys, hiveFallbackApiKeys, freeAccessRoutingMode, allowPaidFallback, revokedCredentialBindings }
    if (needsMigration) this.write(document)
    return document
  }

  private write(document: StoredDocument): void {
    mkdirSync(dirname(this.filePath), { recursive: true })
    const temporary = `${this.filePath}.tmp`
    let descriptor: number | undefined
    try {
      descriptor = openSync(temporary, 'w', 0o600)
      writeFileSync(descriptor, JSON.stringify({ version: STORE_VERSION, ...document }), 'utf8')
      fsyncSync(descriptor)
      closeSync(descriptor)
      descriptor = undefined
      chmodSync(temporary, 0o600)
      renameSync(temporary, this.filePath)
    } catch (error) {
      if (descriptor !== undefined) closeSync(descriptor)
      try { unlinkSync(temporary) } catch { /* no temporary file to clean */ }
      throw error
    }
  }
}

function normalizeHiveProviderKey(provider?: string): string | null {
  if (provider === undefined) return 'rout'
  if (typeof provider !== 'string') return null
  const trimmed = provider.trim().toLowerCase()
  if (trimmed.length === 0) return 'rout'
  return /^[a-z0-9_-]{1,64}$/.test(trimmed) ? trimmed : null
}

function normalizeStoredHiveKeys(raw: unknown): Readonly<Partial<Record<string, string>>> {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {}
  const result: Record<string, string> = {}
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    const providerKey = normalizeHiveProviderKey(key)
    const secret = normalizeStoredSecret(value)
    if (providerKey && secret.length > 0) {
      result[providerKey] = secret
    }
  }
  return result
}

function configuredHiveKeysMap(
  keys: Readonly<Partial<Record<string, string>>>
): Readonly<Partial<Record<string, boolean>>> {
  const result: Record<string, boolean> = {}
  for (const [provider, secret] of Object.entries(keys)) {
    if (typeof secret === 'string' && secret.trim().length > 0) {
      result[provider] = true
    }
  }
  return result
}

function profileFor(document: StoredDocument, provider: ProviderKind): StoredProfile {
  if (provider === 'openai_compatible') {
    return compatibleProfileFor(document, document.profiles[provider]?.profileId ?? 'openai')
  }
  if (provider === 'ollama') {
    return document.profiles[provider] ?? {
      model: '', baseUrl: OLLAMA_DEFAULT_BASE_URL, tier: 'free', secret: '', profileId: 'custom', accountId: '', credentialBinding: '', freeAccessProbePolicy: 'disabled', requestPacing: defaultRequestPacing(provider), modelRateLimits: {}
    }
  }
  return document.profiles[provider] ?? {
    model: '',
    baseUrl: '',
    tier: defaultTierForProfile(provider, 'custom', ''),
    secret: '',
    profileId: 'custom',
    accountId: '',
    credentialBinding: '',
    freeAccessProbePolicy: 'disabled',
    requestPacing: defaultRequestPacing(provider), modelRateLimits: {}
  }
}

function compatibleProfileFor(document: StoredDocument, profileId: ProviderProfileId): StoredProfile {
  const saved = document.compatibleProfiles[profileId]
  if (saved) return saved
  const legacy = document.profiles.openai_compatible
  if (legacy?.profileId === profileId) return legacy
  return {
    model: '',
    baseUrl: providerProfileBaseUrl(profileId, '', '') ?? '',
    tier: defaultTierForProfile('openai_compatible', profileId, ''),
    secret: '',
    profileId,
    accountId: '',
    credentialBinding: '',
    freeAccessProbePolicy: 'disabled',
    requestPacing: defaultRequestPacing('openai_compatible'), modelRateLimits: {}
  }
}

function sameStoredProfile(left: StoredProfile, right: StoredProfile): boolean {
  return left.model === right.model
    && left.baseUrl === right.baseUrl
    && left.tier === right.tier
    && left.secret === right.secret
    && left.profileId === right.profileId
    && left.accountId === right.accountId
    && left.credentialBinding === right.credentialBinding
    && left.freeAccessProbePolicy === right.freeAccessProbePolicy
    && left.requestPacing.enabled === right.requestPacing.enabled
    && left.requestPacing.intervalMs === right.requestPacing.intervalMs
    && left.requestPacing.freeIntervalMs === right.requestPacing.freeIntervalMs
    && JSON.stringify(left.modelRateLimits) === JSON.stringify(right.modelRateLimits)
}

function isFreeAccessProbePolicy(value: unknown): value is FreeAccessProbePolicy {
  return value === 'disabled' || value === 'passive_only' || value === 'on_first_use'
    || value === 'periodic_bounded' || value === 'manual_only'
}

function isFreeAccessRoutingMode(value: unknown): value is FreeAccessRoutingMode {
  return value === 'any' || value === 'prefer_free' || value === 'free_only'
}

function appendRevokedBinding(current: readonly string[], binding: string): readonly string[] {
  const normalized = normalizeCredentialBinding(binding)
  if (normalized === null || current.includes(normalized)) return current
  return [...current, normalized].slice(-MAX_REVOKED_CREDENTIAL_BINDINGS)
}

function newCredentialBinding(): string {
  return `credential:${randomUUID()}`
}

function normalizeCredentialBinding(value: unknown): string | null {
  if (typeof value !== 'string' || !/^credential:[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value)) {
    return null
  }
  return value.toLowerCase()
}

function inferProviderProfileId(baseUrl: string): ProviderProfileId {
  if (baseUrl.trim().length === 0) return 'openai'
  const accountId = cloudflareAccountFromBaseUrl(baseUrl)
  if (accountId !== null) return 'cloudflare_workers_ai'
  for (const [profileId, endpoint] of Object.entries(PROVIDER_PROFILE_ENDPOINTS)) {
    if (endpoint === baseUrl) return profileId as ProviderProfileId
  }
  return 'custom'
}

function cloudflareAccountFromBaseUrl(baseUrl: string): string | null {
  const match = /^https:\/\/api\.cloudflare\.com\/client\/v4\/accounts\/([a-fA-F0-9]{32})\/ai\/v1\/?$/.exec(baseUrl)
  return match?.[1] ?? null
}

function providerProfileBaseUrl(profileId: ProviderProfileId, accountId: string, baseUrl: string): string | null {
  if (profileId === 'cloudflare_workers_ai') {
    return normalizeCloudflareAccountId(accountId) === null
      ? null
      : `https://api.cloudflare.com/client/v4/accounts/${accountId}/ai/v1`
  }
  if (profileId !== 'custom') {
    return PROVIDER_PROFILE_ENDPOINTS[profileId] ?? null
  }
  return normalizeBaseUrl(baseUrl)
}

function configuredProfile(provider: ProviderKind, profile: StoredProfile): boolean {
  return provider === 'ollama' || profile.secret.length > 0
}

/** Secret-free view of one profile; the credential itself never leaves the store. */
function profileSummary(provider: ProviderKind, profile: StoredProfile): ProviderProfileSummary {
  return {
    model: profile.model,
    baseUrl: profile.baseUrl,
    tier: profile.tier,
    configured: configuredProfile(provider, profile),
    profileId: profile.profileId,
    ...(profile.accountId ? { accountId: profile.accountId } : {}),
    freeAccessProbePolicy: profile.freeAccessProbePolicy,
    requestPacing: profile.requestPacing,
    modelRateLimits: profile.modelRateLimits
  }
}

/**
 * Defaults of a vendor profile that has no stored settings yet.
 *
 * The first save of a vendor credential needs a previous profile to compare
 * the probe-consent scope with, and a profile only reaches the stored document
 * through that very save. The shell therefore resolves the missing profile to
 * these defaults instead of rejecting the first key as an unknown profile.
 */
export function defaultCompatibleProfileSummary(profileId: ProviderProfileId): ProviderProfileSummary {
  return profileSummary('openai_compatible', compatibleProfileFor(EMPTY, profileId))
}

function normalizeStoredSecret(value: unknown): string {
  if (typeof value !== 'string' || value.length > MAX_STORED_SECRET_CHARS) return ''
  return /^[A-Za-z0-9+/]*={0,2}$/.test(value) ? value : ''
}

const PAID_COMPATIBLE_PROFILES: ReadonlySet<ProviderProfileId> = new Set([
  'openai',
  'deepseek',
  'xai',
  'together_ai',
  'fireworks_ai',
  'moonshot',
  'zai',
  'nebius',
  'anymodel',
  'custom'
])

/**
 * Infers the default billing tier when a stored profile omits `tier`.
 * Paid providers such as DeepSeek and OpenAI default to `'paid'` unless the
 * model explicitly carries a `:free` suffix.
 */
export function defaultTierForProfile(
  provider: ProviderKind,
  profileId: ProviderProfileId,
  model: string
): ModelTier {
  if (model.toLowerCase().includes(':free')) return 'free'
  if (provider === 'ollama' || provider === 'literouter') return 'free'
  if (provider === 'openai_responses') return 'paid'
  return PAID_COMPATIBLE_PROFILES.has(profileId) ? 'paid' : 'free'
}

function resolveProfileTier(
  rawTier: unknown,
  provider: ProviderKind,
  profileId: ProviderProfileId,
  model: string
): ModelTier {
  if (rawTier === 'paid') return 'paid'
  if (rawTier === 'free') return 'free'
  return defaultTierForProfile(provider, profileId, model)
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
