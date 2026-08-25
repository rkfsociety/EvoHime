import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

import {
  PROVIDER_KINDS,
  type ModelTier,
  type ProviderKind,
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

interface StoredDocument {
  readonly provider: ProviderKind
  readonly model: string
  readonly baseUrl: string
  readonly tier: ModelTier
  readonly secret: string
}

const EMPTY: StoredDocument = {
  provider: 'literouter',
  model: '',
  baseUrl: '',
  tier: 'free',
  secret: ''
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
    return {
      provider: document.provider,
      model: document.model,
      baseUrl: document.baseUrl,
      tier: document.tier,
      configured: document.secret.length > 0
    }
  }

  /**
   * Persists the update. Returns null when the OS refuses to encrypt: the key
   * is dropped rather than written in the clear.
   */
  save(update: ProviderUpdate): ProviderSummary | null {
    const current = this.readDocument()
    let secret = current.secret
    if (update.apiKey.length > 0) {
      if (!this.cipher.isAvailable()) {
        return null
      }
      secret = this.cipher.encrypt(update.apiKey).toString('base64')
    }
    const next: StoredDocument = {
      provider: update.provider,
      model: update.model,
      baseUrl: update.baseUrl,
      tier: update.tier,
      secret
    }
    this.write(next)
    return {
      provider: next.provider,
      model: next.model,
      baseUrl: next.baseUrl,
      tier: next.tier,
      configured: next.secret.length > 0
    }
  }

  /** Forgets the stored key while keeping the provider choice. */
  clearKey(): ProviderSummary {
    const current = this.readDocument()
    this.write({ ...current, secret: '' })
    return {
      provider: current.provider,
      model: current.model,
      baseUrl: current.baseUrl,
      tier: current.tier,
      configured: false
    }
  }

  /**
   * Environment block for the Core process. Only the variables of the selected
   * provider are set, so a stale key of the other one never reaches the model
   * gateway.
   */
  environment(): Record<string, string> {
    const document = this.readDocument()
    const key = this.decryptSecret(document.secret)
    const environment: Record<string, string> = { MODEL_PROVIDER: document.provider }
    if (document.provider === 'openai_compatible' || document.provider === 'openai_responses') {
      if (key) environment['OPENAI_API_KEY'] = key
      if (document.baseUrl) environment['OPENAI_BASE_URL'] = document.baseUrl
      if (document.model) environment['OPENAI_MODEL'] = document.model
      return environment
    }
    if (key) environment['LITEROUTER_API_KEY'] = key
    if (document.baseUrl) environment['LITEROUTER_BASE_URL'] = document.baseUrl
    if (document.model) environment['LITEROUTER_MODEL'] = document.model
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
    const model = normalizeModel(record['model']) ?? ''
    const baseUrl = normalizeBaseUrl(record['baseUrl']) ?? ''
    const secret = typeof record['secret'] === 'string' ? record['secret'] : ''
    const tier: ModelTier = record['tier'] === 'paid' ? 'paid' : 'free'
    return { provider, model, baseUrl, tier, secret }
  }

  private write(document: StoredDocument): void {
    mkdirSync(dirname(this.filePath), { recursive: true })
    const temporary = `${this.filePath}.tmp`
    writeFileSync(temporary, JSON.stringify({ version: STORE_VERSION, ...document }), {
      encoding: 'utf8',
      mode: 0o600
    })
    renameSync(temporary, this.filePath)
  }
}
