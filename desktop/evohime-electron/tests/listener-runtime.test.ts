import { createHash } from 'node:crypto'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  containedPath,
  describeListenerRuntimeError,
  ListenerRuntimeService,
  parseManifest
} from '../src/main/update/listener-runtime'

const REPOSITORY = 'https://github.com/rkfsociety/EvoHime.git'
const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

function tempRoot(name: string): string {
  const root = mkdtempSync(join(tmpdir(), `evohime-listener-${name}-`))
  roots.push(root)
  return root
}

function digest(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex')
}

function fixture(options: { readonly corruptModel?: boolean } = {}) {
  const dll = new TextEncoder().encode('whisper-dll')
  const model = new TextEncoder().encode('ggml-model')
  const manifest = {
    schema: 1,
    version: 'whisper-small-q5_1',
    abi: { name: 'whisper-cpp-full-params-v1', context_params_size: 48, full_params_size: 304 },
    files: [{ role: 'whisper_dll', name: 'whisper.dll', sha256: digest(dll), size: dll.byteLength }],
    models: [
      {
        rung: 'small',
        name: 'models/ggml-small-q5_1.bin',
        sha256: digest(model),
        size: model.byteLength
      }
    ]
  }
  const fetch = vi.fn(async (input: string | URL | Request) => {
    const url = String(input)
    if (url.endsWith('/releases/tags/listener-runtime')) {
      return new Response(
        JSON.stringify({
          assets: [
            { name: 'listener-runtime.json', url: 'https://api.github.com/repos/x/y/releases/assets/manifest' },
            { name: 'whisper.dll', url: 'https://api.github.com/repos/x/y/releases/assets/dll' },
            { name: 'ggml-small-q5_1.bin', url: 'https://api.github.com/repos/x/y/releases/assets/model' }
          ]
        }),
        { status: 200 }
      )
    }
    if (url.endsWith('/manifest')) return new Response(JSON.stringify(manifest), { status: 200 })
    if (url.endsWith('/dll')) return new Response(dll, { status: 200 })
    // Подменённая модель того же размера: размер сходится, хеш — нет.
    return new Response(options.corruptModel ? new TextEncoder().encode('ggml-hackd') : model, {
      status: 200
    })
  })
  return { manifest, fetch, dll, model }
}

function service(toolsDirectory: string, fetch: typeof globalThis.fetch, now?: () => number) {
  return new ListenerRuntimeService({
    toolsDirectory,
    repositoryUrl: REPOSITORY,
    resolveToken: async () => null,
    emit: () => {},
    log: () => {},
    fetch,
    ...(now ? { now } : {})
  })
}

describe('listener runtime manifest', () => {
  it('rejects a manifest that is not the schema this client understands', () => {
    expect(() => parseManifest('{ not json')).toThrow()
    expect(() =>
      parseManifest(
        JSON.stringify({ schema: 2, version: 'v', abi: {}, files: [], models: [{ rung: 'small' }] })
      )
    ).toThrow()
  })

  it('rejects an entry without a real SHA-256', () => {
    const broken = {
      schema: 1,
      version: 'v1',
      abi: { name: 'whisper-cpp-full-params-v1', context_params_size: 48, full_params_size: 304 },
      files: [],
      models: [{ rung: 'small', name: 'model.bin', sha256: 'deadbeef', size: 10 }]
    }
    expect(() => parseManifest(JSON.stringify(broken))).toThrow()
  })

  /** Манифест приходит по сети: путь из него не должен уводить из каталога. */
  it('refuses paths that escape the tools directory', () => {
    const root = tempRoot('contained')
    for (const name of ['../evil.dll', '..\\evil.dll', 'C:\\Windows\\evil.dll', '']) {
      expect(() => containedPath(root, name)).toThrow()
    }
    expect(containedPath(root, 'models/a.bin')).toBe(join(root, 'models', 'a.bin'))
  })
})

describe('listener runtime service', () => {
  it('turns a bare fetch failure into an actionable network message', () => {
    const error = Object.assign(new TypeError('fetch failed'), {
      cause: Object.assign(new Error('getaddrinfo ENOTFOUND api.github.com'), { code: 'ENOTFOUND' })
    })
    expect(describeListenerRuntimeError(error)).toContain('api.github.com')
    expect(describeListenerRuntimeError(error)).not.toBe('fetch failed')
  })

  it('reports a missing runtime instead of pretending it is ready', async () => {
    const root = tempRoot('missing')
    const { fetch } = fixture()
    const status = await service(root, fetch as never).check()
    expect(status.state).toBe('missing')
    expect(status.installedVersion).toBeNull()
    expect(status.availableVersion).toBe('whisper-small-q5_1')
  })

  it('installs a verified set and writes the manifest last', async () => {
    const root = tempRoot('install')
    const { fetch, manifest } = fixture()
    const status = await service(root, fetch as never).download()

    expect(status.state).toBe('ready')
    expect(status.installedVersion).toBe('whisper-small-q5_1')
    expect(existsSync(join(root, 'whisper.dll'))).toBe(true)
    expect(existsSync(join(root, 'models', 'ggml-small-q5_1.bin'))).toBe(true)
    expect(JSON.parse(readFileSync(join(root, 'listener-runtime.json'), 'utf8')).version).toBe(
      manifest.version
    )
    // Staging убирается за собой: недокачанные файлы не остаются на диске.
    expect(existsSync(join(root, '.staging'))).toBe(false)
  })

  /** Совпадения размера мало: подменённый файл ловится именно хешем. */
  it('refuses a tampered file and keeps the previous set in place', async () => {
    const root = tempRoot('tampered')
    writeFileSync(join(root, 'listener-runtime.json'), JSON.stringify({ schema: 1 }))
    const { fetch } = fixture({ corruptModel: true })
    const status = await service(root, fetch as never).download()

    expect(status.state).toBe('failed')
    expect(status.message).toContain('SHA-256')
    // Прежний манифест на месте: переключение не состоялось.
    expect(readFileSync(join(root, 'listener-runtime.json'), 'utf8')).toBe('{"schema":1}')
  })

  it('backs off instead of hammering the release after a failure', async () => {
    const root = tempRoot('backoff')
    const { fetch } = fixture({ corruptModel: true })
    let now = 1_000
    const runtime = service(root, fetch as never, () => now)

    await runtime.download()
    const calls = (fetch as unknown as { mock: { calls: unknown[] } }).mock.calls.length
    const blocked = await runtime.download()

    expect(blocked.message).toContain('Повторная попытка')
    expect((fetch as unknown as { mock: { calls: unknown[] } }).mock.calls.length).toBe(calls)

    now += 60_000
    await runtime.download()
    expect((fetch as unknown as { mock: { calls: unknown[] } }).mock.calls.length).toBeGreaterThan(calls)
  })

  /** Недоступный GitHub не превращает рабочий рантайм в сломанный. */
  it('keeps an installed runtime usable when the release cannot be read', async () => {
    const root = tempRoot('offline')
    const { fetch } = fixture()
    await service(root, fetch as never).download()

    const offline = vi.fn(async () => new Response('', { status: 503 }))
    const status = await service(root, offline as never).check()
    expect(status.state).toBe('ready')
    expect(status.installedVersion).toBe('whisper-small-q5_1')
    expect(status.message).toContain('не удалось')
  })
})
