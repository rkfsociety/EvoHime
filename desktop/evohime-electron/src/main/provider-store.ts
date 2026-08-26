import { chmodSync, closeSync, fsyncSync, mkdirSync, openSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

import {
  PROVIDER_KINDS,
  type ModelTier,
  type ProviderKind,
  type ProviderProfileSummary,
  type ProviderSummary
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
const STORE_VERSION = 1
const MAX_STORED_SECRET_CHARS = 8_192

export interface ProviderUpdate {
  readonly provider: ProviderKind
  /** Empty string keeps the stored key; used when only the model changes. */
  readonly apiKey: string
  readonly model: string
  readonly baseUrl: string
  readonly tier: ModelTier
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
      configured: active.secret.length > 0,
      profiles: Object.fromEntries(PROVIDER_KINDS.map((kind) => {
        const profile = profileFor(document, kind)
        return [kind, {
          model: profile.model,
          baseUrl: profile.baseUrl,
          tier: profile.tier,
          configured: profile.secret.length > 0
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
    const next: StoredDocument = {
      provider: update.provider,
      profiles: {
        ...current.profiles,
        [update.provider]: { model: update.model, baseUrl: update.baseUrl, tier: update.tier, secret }
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
    if (document.provider === 'openai_compatible' || document.provider === 'openai_responses') {
      if (key) environment['OPENAI_API_KEY'] = key
      if (profile.baseUrl) environment['OPENAI_BASE_URL'] = profile.baseUrl
      if (profile.model) environment['OPENAI_MODEL'] = profile.model
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
        profiles[kind] = {
          model: normalizeModel(value['model']) ?? '',
          baseUrl: normalizeBaseUrl(value['baseUrl']) ?? '',
          tier: value['tier'] === 'paid' ? 'paid' : 'free',
          secret: normalizeStoredSecret(value['secret'])
        }
      }
    } else {
      // Version 1 stored one active profile. Preserve it under that provider.
      profiles[provider] = {
        model: normalizeModel(record['model']) ?? '',
        baseUrl: normalizeBaseUrl(record['baseUrl']) ?? '',
        tier: record['tier'] === 'paid' ? 'paid' : 'free',
        secret: normalizeStoredSecret(record['secret'])
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
  return document.profiles[provider] ?? { model: '', baseUrl: '', tier: 'free', secret: '' }
}

function normalizeStoredSecret(value: unknown): string {
  if (typeof value !== 'string' || value.length > MAX_STORED_SECRET_CHARS) return ''
  return /^[A-Za-z0-9+/]*={0,2}$/.test(value) ? value : ''
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
