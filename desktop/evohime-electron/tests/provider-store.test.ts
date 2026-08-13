import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
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
      baseUrl: ''
    })

    expect(summary).toEqual({
      provider: 'literouter',
      model: 'deepseek:free',
      baseUrl: '',
      configured: true
    })
    expect(readFileSync(path, 'utf8')).not.toContain('sk-secret-value')
    expect(JSON.stringify(store.summary())).not.toContain('sk-secret-value')
  })

  it('exports only the selected provider variables', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'openai_compatible', apiKey: 'sk-openai', model: 'gpt-4o-mini', baseUrl: '' })

    expect(store.environment()).toEqual({
      MODEL_PROVIDER: 'openai_compatible',
      OPENAI_API_KEY: 'sk-openai',
      OPENAI_MODEL: 'gpt-4o-mini'
    })
    expect(store.environment()['LITEROUTER_API_KEY']).toBeUndefined()
  })

  it('keeps the stored key when the update carries an empty one', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'literouter', apiKey: 'sk-first', model: 'a', baseUrl: '' })

    const summary = store.save({ provider: 'literouter', apiKey: '', model: 'b', baseUrl: '' })

    expect(summary?.configured).toBe(true)
    expect(store.environment()['LITEROUTER_API_KEY']).toBe('sk-first')
    expect(store.environment()['LITEROUTER_MODEL']).toBe('b')
  })

  it('refuses to store a key when the OS cannot encrypt it', () => {
    const path = storePath()
    const store = new ProviderStore(path, reversibleCipher(false))

    expect(store.save({ provider: 'literouter', apiKey: 'sk-plain', model: '', baseUrl: '' })).toBeNull()
    expect(() => readFileSync(path, 'utf8')).toThrow()
  })

  it('reports a key it cannot decrypt as missing instead of failing', () => {
    const path = storePath()
    new ProviderStore(path, reversibleCipher()).save({
      provider: 'literouter',
      apiKey: 'sk-other-user',
      model: '',
      baseUrl: ''
    })

    const foreign = new ProviderStore(path, {
      isAvailable: () => true,
      encrypt: (value) => Buffer.from(value, 'utf8'),
      decrypt: () => {
        throw new Error('decryption failed')
      }
    })

    expect(foreign.environment()).toEqual({ MODEL_PROVIDER: 'literouter' })
  })

  it('forgets the key but keeps the provider choice', () => {
    const store = new ProviderStore(storePath(), reversibleCipher())
    store.save({ provider: 'openai_compatible', apiKey: 'sk-drop', model: 'm', baseUrl: '' })

    expect(store.clearKey()).toEqual({
      provider: 'openai_compatible',
      model: 'm',
      baseUrl: '',
      configured: false
    })
    expect(store.environment()['OPENAI_API_KEY']).toBeUndefined()
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
