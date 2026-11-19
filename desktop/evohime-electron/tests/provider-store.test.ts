import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import {
  ProviderStore,
  defaultCompatibleProfileSummary,
  normalizeApiKey,
  normalizeBaseUrl,
  normalizeModel,
  normalizeModelRateLimits,
  type SecretCipher
} from '../src/main/provider-store'
import { PROVIDER_PROFILE_ENDPOINTS, PROVIDER_PROFILE_IDS } from '../src/shared/api'

/**
 * The credential store is the only place a provider key is written. These tests
 * pin the properties that keep it safe: the value never lands in the file in the
 * clear, it never crosses back into a summary, and it only reaches the Core
 * environment for the provider it was stored for.
 */

const directories: string[] = []

function storePath(): string {
  const directory = mkdtempSync(join(tmpdir(), 'evohime-provider-'))
  directories.push(directory)
  return join(directory, 'provider.json')
}

/** Reversible stand-in for DPAPI: enough to prove the value is not stored raw. */
function reversibleCipher(available = true): SecretCipher {
  return {
    isAvailable: () => available,
    encrypt: (value) => Buffer.from(`enc:${value}`, 'utf8'),
    decrypt: (value) => {
      const text = value.toString('utf8')
      if (!text.startsWith('enc:')) throw new Error('foreign ciphertext')
      return text.slice(4)
    }
  }
}

afterEach(() => {
  for (const directory of directories.splice(0)) {
    rmSync(directory, { recursive: true, force: true })
  }
})

describe('provider store', () => {
  it('encrypts the key and keeps it out of the summary', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher())

    const summary = store.save({
      provider: 'literouter',
      apiKey: 'sk-secret-value',
      model: 'deepseek:free',
      baseUrl: '',
      tier: 'free'
    })

    expect(summary).toMatchObject({
      provider: 'literouter',
      model: 'deepseek:free',
      baseUrl: '',
      tier: 'free',
      configured: true
    })
    expect(readFileSync(path, 'utf8')).not.toContain('sk-secret-value')
    expect(JSON.stringify(store.summary())).not.toContain('sk-secret-value')
  })

  it('keeps the selected model across restarts without touching the key', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher())
    store.save({
      provider: 'literouter',
      apiKey: 'sk-secret-value',
      model: 'first:free',
      baseUrl: '',
      tier: 'free'
    })

    store.saveSelectedModel('second:free')

    const reopened = new ProviderStore(path, reversibleCipher())
    expect(reopened.summary().model).toBe('second:free')
    expect(reopened.environment()['LITEROUTER_MODEL']).toBe('second:free')
    expect(reopened.environment()['LITEROUTER_API_KEY']).toBe('sk-secret-value')
    expect(readFileSync(path, 'utf8')).not.toContain('sk-secret-value')
  })

  it('exports only the selected provider variables', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'openai_compatible', apiKey: 'sk-openai', model: 'gpt-4o-mini', baseUrl: '', tier: 'paid' })

    expect(store.environment()).toEqual({
      MODEL_PROVIDER: 'openai_compatible',
      MODEL_FREE_ACCESS_ROUTING_MODE: 'any',
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: 'false',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: '',
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'false',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '8000',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '18948',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}',
      OPENAI_API_KEY: 'sk-openai',
      OPENAI_MODEL: 'gpt-4o-mini',
      OPENAI_BASE_URL: 'https://api.openai.com/v1',
      MODEL_PROVIDER_PROFILE_ID: 'openai',
      MODEL_PROVIDER_CREDENTIAL_BINDING: expect.stringMatching(/^credential:[0-9a-f-]{36}$/)
    })
    expect(store.environment()['LITEROUTER_API_KEY']).toBeUndefined()
  })

  it('configures Ollama without a secret and exports only its local endpoint', () => {
    const store = new ProviderStore(storePath(), reversibleCipher(false))
    const summary = store.save({
      provider: 'ollama',
      apiKey: '',
      model: 'qwen3:4b',
      baseUrl: 'http://127.0.0.1:11434/v1',
      tier: 'free'
    })

    expect(summary).toMatchObject({ provider: 'ollama', configured: true, model: 'qwen3:4b' })
    expect(store.environment()).toEqual({
      MODEL_PROVIDER: 'ollama',
      MODEL_FREE_ACCESS_ROUTING_MODE: 'any',
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: 'false',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: '',
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'false',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '8000',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '18948',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}',
      OLLAMA_BASE_URL: 'http://127.0.0.1:11434/v1',
      OLLAMA_MODEL: 'qwen3:4b'
    })
  })

  it('keeps the stored key when the update carries an empty one', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'literouter', apiKey: 'sk-first', model: 'a', baseUrl: '', tier: 'free' })

    const summary = store.save({ provider: 'literouter', apiKey: '', model: 'b', baseUrl: '', tier: 'free' })

    expect(summary?.configured).toBe(true)
    expect(store.environment()['LITEROUTER_API_KEY']).toBe('sk-first')
    expect(store.environment()['LITEROUTER_MODEL']).toBe('b')
  })

  it('keeps API keys and settings isolated per provider', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'literouter', apiKey: 'sk-literouter', model: 'router-model', baseUrl: '', tier: 'free' })
    store.save({ provider: 'openai_compatible', apiKey: 'sk-openai', model: 'gpt-model', baseUrl: '', tier: 'paid' })

    expect(store.environment()).toEqual({
      MODEL_PROVIDER: 'openai_compatible',
      MODEL_FREE_ACCESS_ROUTING_MODE: 'any',
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: 'false',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: '',
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'false',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '8000',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '18948',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}',
      OPENAI_API_KEY: 'sk-openai',
      OPENAI_MODEL: 'gpt-model',
      OPENAI_BASE_URL: 'https://api.openai.com/v1',
      MODEL_PROVIDER_PROFILE_ID: 'openai',
      MODEL_PROVIDER_CREDENTIAL_BINDING: expect.stringMatching(/^credential:[0-9a-f-]{36}$/)
    })
    expect(store.summary().profiles.literouter.configured).toBe(true)

    store.save({ provider: 'literouter', apiKey: '', model: 'router-next', baseUrl: '', tier: 'free' })
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER: 'literouter',
      LITEROUTER_API_KEY: 'sk-literouter',
      LITEROUTER_MODEL: 'router-next',
      MODEL_PROVIDER_CREDENTIAL_BINDING: expect.stringMatching(/^credential:[0-9a-f-]{36}$/)
    })
  })

  it('keeps credentials and settings isolated for each OpenAI-compatible vendor', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher())
    store.save({
      provider: 'openai_compatible', apiKey: 'sk-groq', model: 'llama-groq',
      baseUrl: '', tier: 'paid', profileId: 'groq'
    })
    store.save({
      provider: 'openai_compatible', apiKey: 'sk-deepseek', model: 'deepseek-chat',
      baseUrl: '', tier: 'paid', profileId: 'deepseek'
    })
    const reopened = new ProviderStore(path, reversibleCipher())
    expect(reopened.summary().compatibleProfiles).toMatchObject({
      groq: { configured: true, model: 'llama-groq' },
      deepseek: { configured: true, model: 'deepseek-chat' }
    })

    expect(store.summary().compatibleProfiles).toMatchObject({
      groq: { configured: true, model: 'llama-groq', profileId: 'groq' },
      deepseek: { configured: true, model: 'deepseek-chat', profileId: 'deepseek' }
    })
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER: 'openai_compatible',
      OPENAI_API_KEY: 'sk-deepseek',
      OPENAI_MODEL: 'deepseek-chat',
      MODEL_PROVIDER_PROFILE_ID: 'deepseek'
    })

    reopened.select('openai_compatible', 'groq')
    expect(reopened.environment()).toMatchObject({
      OPENAI_API_KEY: 'sk-groq',
      MODEL_PROVIDER_PROFILE_ID: 'groq'
    })
    expect(reopened.select('openai_compatible', 'deepseek')).toMatchObject({
      profileId: 'deepseek', configured: true, model: 'deepseek-chat'
    })
    expect(reopened.environment()['OPENAI_API_KEY']).toBe('sk-deepseek')

    const selected = store.select('openai_compatible', 'groq')
    expect(selected).toMatchObject({ provider: 'openai_compatible', profileId: 'groq', configured: true })
    expect(store.environment()).toMatchObject({
      OPENAI_API_KEY: 'sk-groq',
      OPENAI_MODEL: 'llama-groq',
      MODEL_PROVIDER_PROFILE_ID: 'groq'
    })

    store.clearKey('openai_compatible', 'deepseek')
    expect(store.summary().compatibleProfiles?.deepseek?.configured).toBe(false)
    expect(store.summary().compatibleProfiles?.groq?.configured).toBe(true)
  })

  it('migrates the old shared OpenAI-compatible profile without losing its key', () => {
    const path = storePath()
    writeFileSync(path, JSON.stringify({
      version: 6,
      provider: 'openai_compatible',
      profiles: {
        openai_compatible: {
          model: 'llama-groq', baseUrl: 'https://api.groq.com/openai/v1', tier: 'paid',
          secret: Buffer.from('enc:sk-groq').toString('base64'), profileId: 'groq', accountId: '',
          credentialBinding: 'credential:11111111-1111-4111-8111-111111111111',
          freeAccessProbePolicy: 'disabled',
          requestPacing: { enabled: false, intervalMs: 8000, freeIntervalMs: 18948 }
        }
      },
      codexModel: '', freeAccessRoutingMode: 'any', allowPaidFallback: false, revokedCredentialBindings: []
    }), 'utf8')

    const store = new ProviderStore(path, reversibleCipher())
    expect(store.summary().compatibleProfiles?.groq).toMatchObject({ configured: true, model: 'llama-groq' })
    expect(store.environment()).toMatchObject({ OPENAI_API_KEY: 'sk-groq', MODEL_PROVIDER_PROFILE_ID: 'groq' })
    expect(JSON.parse(readFileSync(path, 'utf8'))).toMatchObject({
      version: 8,
      profiles: { openai_compatible: { profileId: 'groq', model: 'llama-groq' } },
      compatibleProfiles: { groq: { secret: Buffer.from('enc:sk-groq').toString('base64') } }
    })
  })

  it('switches the active provider without changing its credentials', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'literouter', apiKey: 'sk-literouter', model: 'router-model', baseUrl: '', tier: 'free' })
    store.save({ provider: 'openai_compatible', apiKey: 'sk-openai', model: 'gpt-model', baseUrl: '', tier: 'paid' })

    expect(store.select('literouter')).toMatchObject({ provider: 'literouter', configured: true })
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER: 'literouter',
      LITEROUTER_API_KEY: 'sk-literouter',
      LITEROUTER_MODEL: 'router-model',
      MODEL_PROVIDER_CREDENTIAL_BINDING: expect.stringMatching(/^credential:[0-9a-f-]{36}$/)
    })
    expect(store.summary().profiles.openai_compatible.configured).toBe(true)
  })

  it('persists request pacing per provider and exports it to Core', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({
      provider: 'literouter',
      apiKey: 'sk-literouter',
      model: 'router-model',
      baseUrl: '',
      tier: 'free',
      requestPacing: { enabled: true, intervalMs: 1_500, freeIntervalMs: 2_500 }
    })
    store.save({
      provider: 'openai_compatible',
      apiKey: 'sk-openai',
      model: 'gpt-model',
      baseUrl: '',
      tier: 'paid',
      requestPacing: { enabled: false, intervalMs: 3_500, freeIntervalMs: 4_500 }
    })

    expect(store.summary().profiles.literouter.requestPacing).toEqual({
      enabled: true,
      intervalMs: 1_500,
      freeIntervalMs: 2_500
    })
    expect(store.summary().profiles.openai_compatible.requestPacing).toEqual({
      enabled: false,
      intervalMs: 3_500,
      freeIntervalMs: 4_500
    })

    store.select('literouter')
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'true',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '1500',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '2500',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}'
    })
    store.select('openai_compatible')
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'false',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '3500',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '4500',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}'
    })
  })

  it('keeps per-model limits independent between providers and models', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({
      provider: 'literouter', apiKey: 'lr-key', model: 'model-a', baseUrl: '', tier: 'free',
      modelRateLimits: { 'model-a': { enabled: true, rpm: 5, rph: 50, rpd: 500 } }
    })
    store.save({
      provider: 'openai_compatible', apiKey: 'oa-key', model: 'model-b', baseUrl: '', tier: 'paid',
      modelRateLimits: { 'model-b': { enabled: false, rpm: 0, rph: 0, rpd: 0 } }
    })

    expect(store.summary().profiles.literouter.modelRateLimits).toEqual({
      'model-a': { enabled: true, rpm: 5, rph: 50, rpd: 500 }
    })
    store.select('literouter')
    expect(store.environment()['MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON']).toBe(
      '{"model-a":{"enabled":true,"rpm":5,"rph":50,"rpd":500}}'
    )
    store.select('openai_compatible')
    expect(store.environment()['MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON']).toBe(
      '{"model-b":{"enabled":false,"rpm":0,"rph":0,"rpd":0}}'
    )
  })

  it('keeps request pacing when the key is removed', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({
      provider: 'literouter',
      apiKey: 'sk-literouter',
      model: 'router-model',
      baseUrl: '',
      tier: 'free',
      requestPacing: { enabled: false, intervalMs: 900, freeIntervalMs: 1900 }
    })

    expect(store.clearKey('literouter').profiles.literouter.requestPacing).toEqual({
      enabled: false,
      intervalMs: 900,
      freeIntervalMs: 1900
    })
    expect(store.environment()['MODEL_PROVIDER_REQUEST_PACING_ENABLED']).toBe('false')
  })

  it('rejects an out-of-range request pacing interval', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    expect(store.save({
      provider: 'literouter',
      apiKey: 'sk-literouter',
      model: 'router-model',
      baseUrl: '',
      tier: 'free',
      requestPacing: { enabled: true, intervalMs: 3_600_001, freeIntervalMs: 18_948 }
    })).toBeNull()
    expect(store.save({
      provider: 'literouter',
      apiKey: 'sk-literouter',
      model: 'router-model',
      baseUrl: '',
      tier: 'free',
      requestPacing: { enabled: true, intervalMs: 8_000, freeIntervalMs: -1 }
    })).toBeNull()
  })

  it('exports Responses credentials through the same protected OpenAI environment', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'openai_responses', apiKey: 'sk-responses', model: 'gpt-5-codex', baseUrl: '', tier: 'paid' })

    expect(store.environment()).toEqual({
      MODEL_PROVIDER: 'openai_responses',
      MODEL_FREE_ACCESS_ROUTING_MODE: 'any',
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: 'false',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: '',
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'false',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '8000',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '18948',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}',
      OPENAI_API_KEY: 'sk-responses',
      OPENAI_MODEL: 'gpt-5-codex',
      MODEL_PROVIDER_CREDENTIAL_BINDING: expect.stringMatching(/^credential:[0-9a-f-]{36}$/)
    })
  })

  it('stores a selected Cloudflare profile with a bounded account ID and trusted endpoint', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher())
    const summary = store.save({
      provider: 'openai_compatible',
      apiKey: 'cf-token',
      model: '@cf/meta/llama-3.1-8b-instruct',
      baseUrl: '',
      tier: 'free',
      profileId: 'cloudflare_workers_ai',
      accountId: '0123456789abcdef0123456789abcdef'
    })

    expect(summary).toMatchObject({
      profileId: 'cloudflare_workers_ai',
      accountId: '0123456789abcdef0123456789abcdef',
      baseUrl: 'https://api.cloudflare.com/client/v4/accounts/0123456789abcdef0123456789abcdef/ai/v1'
    })
    expect(store.environment()).toEqual({
      MODEL_PROVIDER: 'openai_compatible',
      MODEL_FREE_ACCESS_ROUTING_MODE: 'any',
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: 'false',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: '',
      MODEL_PROVIDER_REQUEST_PACING_ENABLED: 'false',
      MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS: '8000',
      MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS: '18948',
      MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON: '{}',
      OPENAI_API_KEY: 'cf-token',
      OPENAI_BASE_URL: 'https://api.cloudflare.com/client/v4/accounts/0123456789abcdef0123456789abcdef/ai/v1',
      OPENAI_MODEL: '@cf/meta/llama-3.1-8b-instruct',
      MODEL_PROVIDER_PROFILE_ID: 'cloudflare_workers_ai',
      MODEL_PROVIDER_ACCOUNT_ID: '0123456789abcdef0123456789abcdef',
      MODEL_PROVIDER_CREDENTIAL_BINDING: expect.stringMatching(/^credential:[0-9a-f-]{36}$/)
    })
    expect(readFileSync(path, 'utf8')).not.toContain('cf-token')
  })

  it('refuses to store a key when the OS cannot encrypt it', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher(false))

    expect(
      store.save({ provider: 'literouter', apiKey: 'sk-plain', model: '', baseUrl: '', tier: 'free' })
    ).toBeNull()
    expect(() => readFileSync(path, 'utf8')).toThrow()
  })

  it('rejects an invalid key even when the store is called outside IPC', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher())

    expect(store.save({ provider: 'literouter', apiKey: 'bad\nkey', model: '', baseUrl: '', tier: 'free' })).toBeNull()
    expect(() => readFileSync(path, 'utf8')).toThrow()
  })

  it('treats malformed or oversized persisted ciphertext as missing', () => {
    const path = storePath()
    writeFileSync(path, JSON.stringify({
      version: 1,
      provider: 'literouter',
      profiles: {
        literouter: { model: '', baseUrl: '', tier: 'free', secret: 'not base64!' },
        openai_compatible: { model: '', baseUrl: '', tier: 'free', secret: 'x'.repeat(8_193) }
      },
      codexModel: ''
    }), 'utf8')

    const store = new ProviderStore(path, reversibleCipher())
    expect(store.summary().configured).toBe(false)
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER: 'literouter',
      MODEL_FREE_ACCESS_ROUTING_MODE: 'any',
      MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK: 'false',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: '',
      MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS: ''
    })
  })

  it('cleans up the temporary file when the atomic rename fails', () => {
    const path = storePath()
    mkdirSync(path)
    const store = new ProviderStore(path, reversibleCipher())

    expect(() => store.save({ provider: 'literouter', apiKey: 'sk-value', model: '', baseUrl: '', tier: 'free' })).toThrow()
    expect(() => readFileSync(`${path}.tmp`, 'utf8')).toThrow()
  })

  it('reports a key it cannot decrypt as missing instead of failing', () => {
    const path = storePath()
    new ProviderStore(path, reversibleCipher()).save({
      provider: 'literouter',
      apiKey: 'sk-other-user',
      model: '',
      baseUrl: '',
      tier: 'free'
    })

    const foreign = new ProviderStore(path, {
      isAvailable: () => true,
      encrypt: (value) => Buffer.from(value, 'utf8'),
      decrypt: () => {
        throw new Error('decryption failed')
      }
    })

    expect(foreign.environment()).toMatchObject({
      MODEL_PROVIDER: 'literouter',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'disabled',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: ''
    })
  })

  it('forgets the key but keeps the provider choice', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'openai_compatible', apiKey: 'sk-drop', model: 'm', baseUrl: '', tier: 'paid' })

    expect(store.clearKey()).toMatchObject({
      provider: 'openai_compatible',
      model: 'm',
      baseUrl: 'https://api.openai.com/v1',
      tier: 'paid',
      configured: false
    })
    expect(store.environment()['OPENAI_API_KEY']).toBeUndefined()
    expect(store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']).toBeUndefined()
  })

  it('keeps one stable binding for a key and rotates it only when the key changes', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'literouter', apiKey: 'sk-first', model: 'a', baseUrl: '', tier: 'free' })
    const firstBinding = store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']

    store.save({ provider: 'literouter', apiKey: '', model: 'b', baseUrl: '', tier: 'free' })
    expect(store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']).toBe(firstBinding)
    store.save({ provider: 'literouter', apiKey: 'sk-second', model: 'b', baseUrl: '', tier: 'free' })
    expect(store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']).not.toBe(firstBinding)
  })

  it('migrates legacy credentials to a stable opaque binding without exposing a key', () => {
    const path = storePath()
    writeFileSync(path, JSON.stringify({
      version: 2,
      provider: 'literouter',
      profiles: {
        literouter: { model: 'm', baseUrl: '', tier: 'free', secret: Buffer.from('enc:sk-legacy').toString('base64'), profileId: 'custom', accountId: '' }
      },
      codexModel: ''
    }), 'utf8')

    const store = new ProviderStore(path, reversibleCipher())
    const binding = store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']
    expect(binding).toMatch(/^credential:[0-9a-f-]{36}$/)
    expect(store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']).toBe(binding)
    expect(store.summary().profiles.literouter.requestPacing).toEqual({
      enabled: true,
      intervalMs: 8_000,
      freeIntervalMs: 18_948
    })
    expect(readFileSync(path, 'utf8')).not.toContain('sk-legacy')
    expect(JSON.parse(readFileSync(path, 'utf8'))).toMatchObject({ version: 8 })
  })

  it('binds automatic probe consent to the saved OpenRouter credential and clears it on removal', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    const update = {
      provider: 'openai_compatible' as const,
      apiKey: 'sk-openrouter',
      model: 'author/model:free',
      baseUrl: '',
      tier: 'free' as const,
      profileId: 'openrouter' as const,
      freeAccessProbePolicy: 'on_first_use' as const
    }

    expect(store.save(update)).toBeNull()
    const first = store.save({ ...update, acknowledgeProbePossibleCost: true })
    expect(first?.profiles.openai_compatible.freeAccessProbePolicy).toBe('on_first_use')
    const firstBinding = store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']
    expect(store.environment()).toMatchObject({
      MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY: 'on_first_use',
      MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING: firstBinding
    })

    expect(store.save({ ...update, apiKey: 'sk-replacement' })).toBeNull()
    const rotated = store.save({ ...update, apiKey: 'sk-replacement', acknowledgeProbePossibleCost: true })
    const replacementBinding = store.environment()['MODEL_PROVIDER_CREDENTIAL_BINDING']
    expect(replacementBinding).not.toBe(firstBinding)
    expect(store.environment()['MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING']).toBe(replacementBinding)
    expect(store.environment()['MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS']?.split(',')).toContain(firstBinding)
    expect(rotated?.profiles.openai_compatible.freeAccessProbePolicy).toBe('on_first_use')

    const cleared = store.clearKey()
    expect(cleared.profiles.openai_compatible.freeAccessProbePolicy).toBe('disabled')
    expect(store.environment()['MODEL_PROVIDER_FREE_ACCESS_PROBE_POLICY']).toBe('disabled')
    expect(store.environment()['MODEL_PROVIDER_FREE_ACCESS_PROBE_CONSENT_BINDING']).toBe('')
    expect(store.environment()['MODEL_PROVIDER_REVOKED_CREDENTIAL_BINDINGS']?.split(',')).toContain(replacementBinding)
  })
})

describe('provider profile endpoints', () => {
  it('stores every fixed vendor profile with its trusted endpoint', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    const fixedProfiles = PROVIDER_PROFILE_IDS.filter(
      (profileId) => profileId !== 'custom' && profileId !== 'cloudflare_workers_ai'
    )

    for (const profileId of fixedProfiles) {
      const summary = store.save({
        provider: 'openai_compatible',
        apiKey: 'sk-test',
        model: 'provider-model',
        baseUrl: '',
        tier: 'paid',
        profileId
      })
      expect(summary?.profiles.openai_compatible.profileId).toBe(profileId)
      expect(summary?.profiles.openai_compatible.baseUrl).toBe(PROVIDER_PROFILE_ENDPOINTS[profileId])
    }
  })

  it('describes an unsaved vendor profile with its defaults', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())

    // Nothing is stored yet, so the summary deliberately lists no vendor
    // profile; the shell falls back to these defaults for the first save.
    expect(store.summary().compatibleProfiles).toEqual({})
    expect(defaultCompatibleProfileSummary('deepseek')).toEqual({
      model: '',
      baseUrl: 'https://api.deepseek.com/v1',
      tier: 'paid',
      configured: false,
      profileId: 'deepseek',
      freeAccessProbePolicy: 'disabled',
      requestPacing: { enabled: false, intervalMs: 8_000, freeIntervalMs: 18_948 },
      modelRateLimits: {}
    })
    expect(defaultCompatibleProfileSummary('cloudflare_workers_ai')).toMatchObject({
      baseUrl: '',
      configured: false
    })
  })
})

describe('provider input bounds', () => {
  it('validates bounded per-model rate limits', () => {
    expect(normalizeModelRateLimits({ 'gpt-4o': { enabled: true, rpm: 10, rph: 100, rpd: 500 } })).toEqual({
      'gpt-4o': { enabled: true, rpm: 10, rph: 100, rpd: 500 }
    })
    expect(normalizeModelRateLimits({ 'gpt-4o': { enabled: true, rpm: -1, rph: 0, rpd: 0 } })).toBeNull()
    expect(normalizeModelRateLimits({ 'gpt-4o': { enabled: true, rpm: 10_001, rph: 0, rpd: 0 } })).toBeNull()
    expect(normalizeModelRateLimits(Object.fromEntries(
      Array.from({ length: 65 }, (_, index) => [`model-${index}`, { enabled: false, rpm: 0, rph: 0, rpd: 0 }])
    ))).toBeNull()
  })

  it('accepts https and loopback http only', () => {
    expect(normalizeBaseUrl('https://api.literouter.com/v1')).toBe('https://api.literouter.com/v1')
    expect(normalizeBaseUrl('http://localhost:1234/v1')).toBe('http://localhost:1234/v1')
    expect(normalizeBaseUrl('')).toBe('')
    // A plain-http remote host would put the key on the wire in the clear.
    expect(normalizeBaseUrl('http://example.com/v1')).toBeNull()
    expect(normalizeBaseUrl('ftp://example.com')).toBeNull()
    expect(normalizeBaseUrl('not a url')).toBeNull()
  })

  it('rejects a key that could break the environment block', () => {
    expect(normalizeApiKey(' sk-value ')).toBe('sk-value')
    expect(normalizeApiKey('sk\nMODEL_PROVIDER=mock')).toBeNull()
    expect(normalizeApiKey('x'.repeat(513))).toBeNull()
  })

  it('rejects a model identifier with whitespace', () => {
    expect(normalizeModel('deepseek:free')).toBe('deepseek:free')
    expect(normalizeModel('')).toBe('')
    expect(normalizeModel('two words')).toBeNull()
  })

  it('deepseek_defaults_to_paid_tier', () => {
    const path = storePath()
    writeFileSync(path, JSON.stringify({
      version: 8,
      provider: 'openai_compatible',
      profiles: {
        openai_compatible: {
          model: 'deepseek-chat',
          baseUrl: 'https://api.deepseek.com/v1',
          secret: Buffer.from('enc:sk-ds').toString('base64'),
          profileId: 'deepseek'
        },
        openai_responses: {
          model: 'gpt-5-codex',
          baseUrl: '',
          secret: Buffer.from('enc:sk-oa').toString('base64')
        }
      },
      compatibleProfiles: {
        deepseek: {
          model: 'deepseek-chat',
          baseUrl: 'https://api.deepseek.com/v1',
          secret: Buffer.from('enc:sk-ds').toString('base64'),
          profileId: 'deepseek'
        },
        openrouter: {
          model: 'deepseek/deepseek-r1:free',
          baseUrl: 'https://openrouter.ai/api/v1',
          secret: Buffer.from('enc:sk-or').toString('base64'),
          profileId: 'openrouter'
        }
      }
    }), 'utf8')

    const store = new ProviderStore(path, reversibleCipher())
    const summary = store.summary()
    expect(summary.tier).toBe('paid')
    expect(summary.compatibleProfiles?.deepseek?.tier).toBe('paid')
    expect(summary.profiles.openai_responses.tier).toBe('paid')
    expect(summary.compatibleProfiles?.openrouter?.tier).toBe('free')
    expect(defaultCompatibleProfileSummary('deepseek').tier).toBe('paid')
    expect(defaultCompatibleProfileSummary('openai').tier).toBe('paid')
  })

  it('migrates legacy compatibleProfiles.routmy secret to rout and exports ROUT_API_KEY', () => {
    const path = storePath()
    writeFileSync(path, JSON.stringify({
      version: 8,
      provider: 'literouter',
      profiles: {
        literouter: {
          model: 'gpt-4o-mini:free',
          baseUrl: '',
          tier: 'free',
          secret: Buffer.from('enc:sk-lr').toString('base64'),
          profileId: 'custom'
        }
      },
      compatibleProfiles: {
        routmy: {
          model: 'deepseek-chat',
          baseUrl: 'https://api.rout.my/v1',
          tier: 'free',
          secret: Buffer.from('enc:sk-routmy-legacy').toString('base64'),
          profileId: 'routmy'
        }
      }
    }), 'utf8')

    const store = new ProviderStore(path, reversibleCipher())
    const summary = store.summary()
    expect(summary.compatibleProfiles?.rout?.configured).toBe(true)
    expect(summary.compatibleProfiles?.rout?.tier).toBe('free')
    expect(summary.compatibleProfiles?.rout?.model).toBe('deepseek-chat')
    expect((summary.compatibleProfiles as Record<string, unknown>)['routmy']).toBeUndefined()

    const env = store.environment()
    expect(env['ROUT_API_KEY']).toBe('sk-routmy-legacy')
    expect(env['ROUT_BASE_URL']).toBe('https://api.rout.my/v1')
    expect(env['ROUT_MODEL']).toBe('deepseek-chat')
    expect(env['ROUTMY_API_KEY']).toBeUndefined()

    const persisted = JSON.parse(readFileSync(path, 'utf8'))
    expect(persisted.compatibleProfiles.routmy).toBeUndefined()
    expect(persisted.compatibleProfiles.rout.profileId).toBe('rout')
  })

  it('encrypts hive_api_key and exports HIVE_API_KEY only when set', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher())

    expect(store.environment()['HIVE_API_KEY']).toBeUndefined()
    expect(store.saveHiveApiKey('sk-hive-only')).toBe(true)
    expect(store.environment()['HIVE_API_KEY']).toBe('sk-hive-only')
    expect(readFileSync(path, 'utf8')).not.toContain('sk-hive-only')

    expect(store.saveHiveApiKey('')).toBe(true)
    expect(store.environment()['HIVE_API_KEY']).toBeUndefined()

    expect(store.environment()['HIVE_FALLBACK_API_KEY']).toBeUndefined()
    expect(store.saveHiveFallbackApiKey('sk-hive-fallback-only')).toBe(true)
    expect(store.environment()['HIVE_FALLBACK_API_KEY']).toBe('sk-hive-fallback-only')
    expect(readFileSync(path, 'utf8')).not.toContain('sk-hive-fallback-only')

    expect(store.saveHiveFallbackApiKey('')).toBe(true)
    expect(store.environment()['HIVE_FALLBACK_API_KEY']).toBeUndefined()
  })
})
