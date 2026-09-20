import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { zipSync } from 'fflate'

import { afterEach, describe, expect, it, vi } from 'vitest'

import { downloadModuleRelease, downloadReleaseComponents, downloadReleaseInstaller } from '../src/main/update/release-installer'

const COMMIT = 'a'.repeat(40)
const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

function fixture() {
  const bytes = new TextEncoder().encode('installer')
  const manifest = JSON.stringify({
    version: 1,
    product: 'EvoHime',
    asset: 'EvoHime-Setup.exe',
    commit: COMMIT,
    branch: 'main',
    size: bytes.byteLength,
    sha256: createHash('sha256').update(bytes).digest('hex')
  })
  const fetch = vi.fn(async (input: string | URL | Request) => {
    const url = String(input)
    if (url.endsWith('/releases/tags/installer')) {
      return new Response(JSON.stringify({ assets: [
        { name: 'EvoHime-Setup.json', url: 'https://api.github.com/repos/rkfsociety/EvoHime/releases/assets/manifest' },
        { name: 'EvoHime-Setup.exe', url: 'https://api.github.com/repos/rkfsociety/EvoHime/releases/assets/installer' }
      ] }), { status: 200 })
    }
    if (url.endsWith('/manifest')) return new Response(manifest, { status: 200 })
    return new Response(bytes, { status: 200 })
  })
  return { bytes, fetch }
}

describe('release installer', () => {
  it('downloads only an installer whose manifest matches the green commit', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-release-'))
    roots.push(root)
    const { fetch } = fixture()

    const result = await downloadReleaseInstaller(
      'https://github.com/rkfsociety/EvoHime.git', 'main', COMMIT, root, null, { fetch }
    )

    expect(result.marker.commit).toBe(COMMIT)
    expect(fetch).toHaveBeenCalledTimes(3)
  })

  it('rejects a stale release instead of installing it', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-release-stale-'))
    roots.push(root)
    const { fetch } = fixture()

    await expect(downloadReleaseInstaller(
      'https://github.com/rkfsociety/EvoHime.git', 'main', 'b'.repeat(40), root, null, { fetch }
    )).rejects.toThrow('манифест относится')
  })

  it('downloads only the selected component and verifies its digest', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-components-'))
    roots.push(root)
    const bytes = zipSync({ 'ui-bundle/index.html': new TextEncoder().encode('ui bundle') })
    const hash = createHash('sha256').update(bytes).digest('hex')
    const manifest = JSON.stringify({
      schema: 'evohime.component-manifest.v1', release_commit: COMMIT,
      components: [{ id: 'ui-bundle', version: '1.0.0', artifact: 'ui.zip', path: 'ui.zip', size: bytes.byteLength, sha256: hash, required: true, protocol: 'desktop-ipc-v1' }]
    })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases/tags/installer')) return new Response(JSON.stringify({ assets: [
        { name: 'evohime.components.json', url: 'https://api.github.com/repos/rkfsociety/EvoHime/releases/assets/components' },
        { name: 'ui.zip', url: 'https://api.github.com/repos/rkfsociety/EvoHime/releases/assets/ui' }
      ] }), { status: 200 })
      if (url.endsWith('/components')) return new Response(manifest, { status: 200 })
      return new Response(bytes, { status: 200 })
    })
    const result = await downloadReleaseComponents('https://github.com/rkfsociety/EvoHime.git', root, ['ui-bundle'], null, { fetch })
    expect(result.selected).toEqual(['ui-bundle'])
    expect(result.files).toEqual([join(root, 'ui.zip')])
    expect(existsSync(join(root, 'ui-bundle', 'index.html'))).toBe(true)
    expect(fetch).toHaveBeenCalledTimes(3)
  })

  it('rejects an oversized UI archive before downloading its bytes', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-components-oversized-'))
    roots.push(root)
    const manifest = JSON.stringify({
      schema: 'evohime.component-manifest.v1', release_commit: COMMIT,
      components: [{ id: 'ui-bundle', version: '1.0.0', artifact: 'ui.zip', path: 'ui.zip', size: 128 * 1024 * 1024 + 1, sha256: '0'.repeat(64), required: true }]
    })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases/tags/installer')) return new Response(JSON.stringify({ assets: [
        { name: 'evohime.components.json', url: 'https://api.github.com/repos/x/y/releases/assets/components' },
        { name: 'ui.zip', url: 'https://api.github.com/repos/x/y/releases/assets/ui' }
      ] }), { status: 200 })
      if (url.endsWith('/components')) return new Response(manifest, { status: 200 })
      throw new Error('UI bytes must not be downloaded')
    })

    await expect(downloadReleaseComponents('https://github.com/rkfsociety/EvoHime.git', root, ['ui-bundle'], null, { fetch }))
      .rejects.toThrow('too large before extraction')
    expect(fetch).toHaveBeenCalledTimes(2)
  })

  it('rejects duplicate component identities before downloading artifacts', async () => {
    const cases = [
      ['id', [
        { id: 'ui-bundle', artifact: 'ui-a.zip', path: 'ui-a.zip' },
        { id: 'ui-bundle', artifact: 'ui-b.zip', path: 'ui-b.zip' }
      ]],
      ['artifact', [
        { id: 'ui-bundle', artifact: 'same.zip', path: 'ui-a.zip' },
        { id: 'core', artifact: 'same.zip', path: 'core.exe' }
      ]],
      ['path', [
        { id: 'ui-bundle', artifact: 'ui.zip', path: 'same/path' },
        { id: 'core', artifact: 'core.exe', path: 'same/path' }
      ]]
    ] as const

    for (const [kind, identities] of cases) {
      const root = mkdtempSync(join(tmpdir(), `evohime-components-duplicate-${kind}-`))
      roots.push(root)
      const manifest = JSON.stringify({
        schema: 'evohime.component-manifest.v1', release_commit: COMMIT,
        components: identities.map((component) => ({
          ...component, version: '1.0.0', size: 1, sha256: '0'.repeat(64), required: true
        }))
      })
      const fetch = vi.fn(async (input: string | URL | Request) => {
        const url = String(input)
        if (url.endsWith('/releases/tags/installer')) return new Response(JSON.stringify({ assets: [
          { name: 'evohime.components.json', url: 'https://api.github.com/repos/x/y/releases/assets/components' }
        ] }), { status: 200 })
        if (url.endsWith('/components')) return new Response(manifest, { status: 200 })
        throw new Error('component artifact must not be downloaded')
      })

      await expect(downloadReleaseComponents(
        'https://github.com/rkfsociety/EvoHime.git', root, [...new Set(identities.map((component) => component.id))], null, { fetch }
      )).rejects.toThrow('duplicate component')
      expect(fetch).toHaveBeenCalledTimes(2)
    }
  })

  it('rejects duplicate selected component ids before contacting GitHub', async () => {
    const fetch = vi.fn()
    await expect(downloadReleaseComponents(
      'https://github.com/rkfsociety/EvoHime.git', tmpdir(), ['ui-bundle', 'ui-bundle'], null, { fetch }
    )).rejects.toThrow('selected component set')
    expect(fetch).not.toHaveBeenCalled()
  })

  it('preserves the previous UI bundle when archive extraction is rejected', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-components-unsafe-'))
    roots.push(root)
    mkdirSync(join(root, 'ui-bundle'), { recursive: true })
    writeFileSync(join(root, 'ui-bundle', 'old.html'), 'previous')
    const bytes = zipSync({
      'ui-bundle/index.html': new TextEncoder().encode('new'),
      'ui-bundle/../escape.js': new TextEncoder().encode('unsafe')
    })
    const manifest = JSON.stringify({
      schema: 'evohime.component-manifest.v1', release_commit: COMMIT,
      components: [{ id: 'ui-bundle', version: '1.0.0', artifact: 'ui.zip', path: 'ui.zip', size: bytes.byteLength, sha256: createHash('sha256').update(bytes).digest('hex'), required: true }]
    })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases/tags/installer')) return new Response(JSON.stringify({ assets: [
        { name: 'evohime.components.json', url: 'https://api.github.com/repos/x/y/releases/assets/components' },
        { name: 'ui.zip', url: 'https://api.github.com/repos/x/y/releases/assets/ui' }
      ] }), { status: 200 })
      if (url.endsWith('/components')) return new Response(manifest, { status: 200 })
      return new Response(bytes, { status: 200 })
    })

    await expect(downloadReleaseComponents('https://github.com/rkfsociety/EvoHime.git', root, ['ui-bundle'], null, { fetch }))
      .rejects.toThrow('unsafe UI archive path')
    expect(existsSync(join(root, 'ui-bundle', 'old.html'))).toBe(true)
    expect(existsSync(join(root, 'escape.js'))).toBe(false)
  })

  it('downloads a module from its own versioned release', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-'))
    roots.push(root)
    const bytes = new TextEncoder().encode('core module')
    const hash = createHash('sha256').update(bytes).digest('hex')
    const manifest = JSON.stringify({ schema: 'evohime.module-release.v1', module: 'core', version: '2.1.0', artifact: 'evohime-core.exe', size: bytes.length, sha256: hash })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases?per_page=100')) return new Response(JSON.stringify([{ tag_name: 'module-core-v2.1.0', assets: [
        { name: 'core.manifest.json', url: 'https://api.github.com/repos/x/y/releases/assets/core-manifest' },
        { name: 'evohime-core.exe', url: 'https://api.github.com/repos/x/y/releases/assets/core' }
      ] }]), { status: 200 })
      if (url.endsWith('core-manifest')) return new Response(manifest, { status: 200 })
      return new Response(bytes, { status: 200 })
    })
    const result = await downloadModuleRelease('https://github.com/rkfsociety/EvoHime.git', 'core', root, null, { fetch })
    expect(result.manifest.version).toBe('2.1.0')
    expect(result.file).toBe(join(root, 'evohime-core.exe'))
  })

  it('ignores malformed module release tags when selecting the newest version', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-tags-'))
    roots.push(root)
    const bytes = new TextEncoder().encode('valid module')
    const hash = createHash('sha256').update(bytes).digest('hex')
    const manifest = JSON.stringify({ schema: 'evohime.module-release.v1', module: 'core', version: '2.2.0', artifact: 'evohime-core.exe', size: bytes.length, sha256: hash })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases?per_page=100')) return new Response(JSON.stringify([
        { tag_name: 'module-core-vnot-a-version', assets: [] },
        { tag_name: 'module-core-v2.2.0', assets: [
          { name: 'core.manifest.json', url: 'https://api.github.com/repos/x/y/releases/assets/core-manifest' },
          { name: 'evohime-core.exe', url: 'https://api.github.com/repos/x/y/releases/assets/core' }
        ] }
      ]), { status: 200 })
      if (url.endsWith('core-manifest')) return new Response(manifest, { status: 200 })
      return new Response(bytes, { status: 200 })
    })

    const result = await downloadModuleRelease('https://github.com/rkfsociety/EvoHime.git', 'core', root, null, { fetch })
    expect(result.manifest.version).toBe('2.2.0')
  })

  it('follows bounded GitHub release pagination when selecting a module', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-pagination-'))
    roots.push(root)
    const bytes = new TextEncoder().encode('paged module')
    const hash = createHash('sha256').update(bytes).digest('hex')
    const manifest = JSON.stringify({ schema: 'evohime.module-release.v1', module: 'core', version: '2.3.0', artifact: 'evohime-core.exe', size: bytes.length, sha256: hash })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases?per_page=100')) return new Response(JSON.stringify([
        { tag_name: 'module-core-v2.2.0', assets: [] }
      ]), {
        status: 200,
        headers: { link: '<https://api.github.com/repos/x/y/releases?per_page=100&page=2>; rel="next"' }
      })
      if (url.endsWith('/releases?per_page=100&page=2')) return new Response(JSON.stringify([
        { tag_name: 'module-core-v2.3.0', assets: [
          { name: 'core.manifest.json', url: 'https://api.github.com/repos/x/y/releases/assets/core-manifest' },
          { name: 'evohime-core.exe', url: 'https://api.github.com/repos/x/y/releases/assets/core' }
        ] }
      ]), { status: 200 })
      if (url.endsWith('core-manifest')) return new Response(manifest, { status: 200 })
      return new Response(bytes, { status: 200 })
    })

    const result = await downloadModuleRelease('https://github.com/rkfsociety/EvoHime.git', 'core', root, null, { fetch })
    expect(result.manifest.version).toBe('2.3.0')
    expect(fetch).toHaveBeenCalledTimes(4)
  })

  it('does not publish an artifact when its digest is invalid', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-invalid-digest-'))
    roots.push(root)
    const bytes = new TextEncoder().encode('tampered module')
    const manifest = JSON.stringify({ schema: 'evohime.module-release.v1', module: 'core', version: '2.1.1', artifact: 'evohime-core.exe', size: bytes.length, sha256: '0'.repeat(64) })
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/releases?per_page=100')) return new Response(JSON.stringify([{ tag_name: 'module-core-v2.1.1', assets: [
        { name: 'core.manifest.json', url: 'https://api.github.com/repos/x/y/releases/assets/core-manifest' },
        { name: 'evohime-core.exe', url: 'https://api.github.com/repos/x/y/releases/assets/core' }
      ] }]), { status: 200 })
      if (url.endsWith('core-manifest')) return new Response(manifest, { status: 200 })
      return new Response(bytes, { status: 200 })
    })

    await expect(downloadModuleRelease('https://github.com/rkfsociety/EvoHime.git', 'core', root, null, { fetch }))
      .rejects.toThrow('SHA-256')
    expect(() => statSync(join(root, 'evohime-core.exe'))).toThrow()
  })
})
