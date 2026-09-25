import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import {
  ProviderStore,
  normalizeApiKey,
  normalizeBaseUrl,
  normalizeModel,
  type SecretCipher
} from '../src/main/provider-store'

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
    expect(readFileSync(path, 'utf8')).not.toContain('sk-legacy')
    expect(JSON.parse(readFileSync(path, 'utf8'))).toMatchObject({ version: 5 })
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

describe('provider input bounds', () => {
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
})
