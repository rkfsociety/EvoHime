import { chmodSync, closeSync, fsyncSync, mkdirSync, openSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

import {
  PROVIDER_KINDS,
  PROVIDER_PROFILE_ENDPOINTS,
  PROVIDER_PROFILE_IDS,
  type ModelTier,
  type ProviderKind,
  type ProviderProfileId,
  type ProviderProfileSummary,
  type ProviderSummary,
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
const STORE_VERSION = 2
const MAX_STORED_SECRET_CHARS = 8_192

export interface ProviderUpdate {
  readonly provider: ProviderKind
  /** Empty string keeps the stored key; used when only the model changes. */
  readonly apiKey: string
  readonly model: string
  readonly baseUrl: string
  readonly tier: ModelTier
  readonly profileId?: ProviderProfileId
  readonly accountId?: string
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
}

interface StoredDocument {
  readonly provider: ProviderKind
  readonly profiles: Readonly<Partial<Record<ProviderKind, StoredProfile>>>
  readonly codexModel: string
}

const EMPTY: StoredDocument = {
  provider: 'literouter',
  profiles: {},
  codexModel: ''
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
    return {
      provider: document.provider,
      model: active.model,
      baseUrl: active.baseUrl,
      tier: active.tier,
      configured: configuredProfile(document.provider, active),
      profileId: active.profileId,
      ...(active.accountId ? { accountId: active.accountId } : {}),
      profiles: Object.fromEntries(PROVIDER_KINDS.map((kind) => {
        const profile = profileFor(document, kind)
        return [kind, {
          model: profile.model,
          baseUrl: profile.baseUrl,
          tier: profile.tier,
          configured: configuredProfile(kind, profile),
          profileId: profile.profileId,
          ...(profile.accountId ? { accountId: profile.accountId } : {})
        } satisfies ProviderProfileSummary]
      })) as Readonly<Record<ProviderKind, ProviderProfileSummary>>
    }
  }

  /**
   * Persists the update. Returns null when the OS refuses to encrypt: the key
   * is dropped rather than written in the clear.
   */
  save(update: ProviderUpdate): ProviderSummary | null {
    const current = this.readDocument()
    const previous = profileFor(current, update.provider)
    const requestedKey = normalizeApiKey(update.apiKey)
    if (requestedKey === null) return null
    let secret = previous.secret
    if (requestedKey.length > 0) {
      if (!this.cipher.isAvailable()) {
        return null
      }
      secret = this.cipher.encrypt(requestedKey).toString('base64')
    }
    const profileId = update.provider === 'openai_compatible'
      ? update.profileId ?? inferProviderProfileId(update.baseUrl)
      : 'custom'
    const accountId = profileId === 'cloudflare_workers_ai'
      ? normalizeCloudflareAccountId(update.accountId) ?? cloudflareAccountFromBaseUrl(update.baseUrl) ?? ''
      : ''
    const baseUrl = update.provider === 'openai_compatible'
      ? providerProfileBaseUrl(profileId, accountId, update.baseUrl)
      : normalizeBaseUrl(update.baseUrl)
    if (baseUrl === null || (profileId === 'cloudflare_workers_ai' && accountId.length === 0)) {
      return null
    }
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
          accountId
        }
      },
      codexModel: current.codexModel
    }
    this.write(next)
    return this.summaryFor(next)
  }

  /** Changes the active API profile without touching any stored credential. */
  select(provider: ProviderKind): ProviderSummary {
    const current = this.readDocument()
    if (current.provider === provider) return this.summaryFor(current)
    const next = { ...current, provider }
    this.write(next)
    return this.summaryFor(next)
  }

  /** Forgets the stored key while keeping the provider choice. */
  clearKey(provider = this.readDocument().provider): ProviderSummary {
    const current = this.readDocument()
    const active = profileFor(current, provider)
    const next = { ...current, provider, profiles: { ...current.profiles, [provider]: { ...active, secret: '' } } }
    this.write(next)
    return this.summaryFor(next)
  }

  codexModel(): string {
    return this.readDocument().codexModel
  }

  saveCodexModel(model: string): void {
    const current = this.readDocument()
    this.write({ ...current, codexModel: model })
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
    const environment: Record<string, string> = { MODEL_PROVIDER: document.provider }
    if (document.codexModel) environment['CODEX_MODEL'] = document.codexModel
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
      return environment
    }
    if (key) environment['LITEROUTER_API_KEY'] = key
    if (profile.baseUrl) environment['LITEROUTER_BASE_URL'] = profile.baseUrl
    if (profile.model) environment['LITEROUTER_MODEL'] = profile.model
    return environment
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
    if (isRecord(record['profiles'])) {
      for (const kind of PROVIDER_KINDS) {
        const value = record['profiles'][kind]
        if (!isRecord(value)) continue
        const baseUrl = normalizeBaseUrl(value['baseUrl']) ?? ''
        const profileId = kind === 'openai_compatible'
          ? (isProviderProfileId(value['profileId']) ? value['profileId'] : inferProviderProfileId(baseUrl))
          : 'custom'
        const accountId = profileId === 'cloudflare_workers_ai'
          ? normalizeCloudflareAccountId(value['accountId']) ?? cloudflareAccountFromBaseUrl(baseUrl) ?? ''
          : ''
        profiles[kind] = {
          model: normalizeModel(value['model']) ?? '',
          baseUrl: providerProfileBaseUrl(profileId, accountId, baseUrl) ?? '',
          tier: value['tier'] === 'paid' ? 'paid' : 'free',
          secret: normalizeStoredSecret(value['secret']),
          profileId,
          accountId
        }
      }
    } else {
      // Version 1 stored one active profile. Preserve it under that provider.
      profiles[provider] = {
        model: normalizeModel(record['model']) ?? '',
        baseUrl: normalizeBaseUrl(record['baseUrl']) ?? '',
        tier: record['tier'] === 'paid' ? 'paid' : 'free',
        secret: normalizeStoredSecret(record['secret']),
        profileId: provider === 'openai_compatible'
          ? inferProviderProfileId(normalizeBaseUrl(record['baseUrl']) ?? '')
          : 'custom',
        accountId: cloudflareAccountFromBaseUrl(normalizeBaseUrl(record['baseUrl']) ?? '') ?? ''
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
    const codexModel = normalizeModel(record['codexModel']) ?? ''
    return { provider, profiles, codexModel }
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

function profileFor(document: StoredDocument, provider: ProviderKind): StoredProfile {
  if (provider === 'ollama') {
    return document.profiles[provider] ?? {
      model: '', baseUrl: OLLAMA_DEFAULT_BASE_URL, tier: 'free', secret: '', profileId: 'custom', accountId: ''
    }
  }
  return document.profiles[provider] ?? {
    model: '',
    baseUrl: provider === 'openai_compatible' ? PROVIDER_PROFILE_ENDPOINTS.openai ?? '' : '',
    tier: 'free',
    secret: '',
    profileId: provider === 'openai_compatible' ? 'openai' : 'custom',
    accountId: ''
  }
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

function normalizeStoredSecret(value: unknown): string {
  if (typeof value !== 'string' || value.length > MAX_STORED_SECRET_CHARS) return ''
  return /^[A-Za-z0-9+/]*={0,2}$/.test(value) ? value : ''
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
