import { createHash } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { zipSync } from 'fflate'

import { afterEach, describe, expect, it, vi } from 'vitest'

import { downloadReleaseComponents, downloadReleaseInstaller } from '../src/main/update/release-installer'

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
    const bytes = zipSync({ 'index.html': new TextEncoder().encode('ui bundle') })
    const hash = createHash('sha256').update(bytes).digest('hex')
    const manifest = JSON.stringify({
      schema: 'evohime.component-manifest.v1', release_commit: COMMIT,
      components: [{ id: 'ui-bundle', version: '1', artifact: 'ui.zip', path: 'ui.zip', size: bytes.byteLength, sha256: hash, required: true }]
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
    const result = await downloadReleaseComponents('https://github.com/rkfsociety/EvoHime.git', COMMIT, root, ['ui-bundle'], null, { fetch })
    expect(result.selected).toEqual(['ui-bundle'])
    expect(result.files).toEqual([join(root, 'ui.zip')])
    expect(fetch).toHaveBeenCalledTimes(3)
  })
})
