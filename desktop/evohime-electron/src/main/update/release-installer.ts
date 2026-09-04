import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { unzipSync } from 'fflate'

import { normalizeCommit, type BuildMarker } from './config'
import { githubApiBase } from './commit-status'

const RELEASE_TAG = 'installer'
const INSTALLER_ASSET = 'EvoHime-Setup.exe'
const MANIFEST_ASSET = 'EvoHime-Setup.json'
const COMPONENT_MANIFEST_ASSET = 'evohime.components.json'
const MAX_MANIFEST_BYTES = 64 * 1024
const MAX_INSTALLER_BYTES = 2 * 1024 * 1024 * 1024
const MAX_UI_FILES = 512
const MAX_UI_BYTES = 512 * 1024 * 1024
const REQUEST_TIMEOUT_MS = 120_000

export interface ReleaseInstallerManifest {
  readonly commit: string
  readonly branch: string
  readonly asset: string
  readonly sha256: string
  readonly size: number
}

export interface DownloadedInstaller {
  readonly installer: string
  readonly marker: BuildMarker
}

export interface ReleaseComponentManifest {
  readonly schema: 'evohime.component-manifest.v1'
  readonly release_commit: string
  readonly components: readonly {
    readonly id: string
    readonly artifact: string
    readonly path: string
    readonly size: number
    readonly sha256: string
    readonly required: boolean
  }[]
}

export interface DownloadedComponents {
  readonly manifest: ReleaseComponentManifest
  readonly selected: readonly string[]
  readonly files: readonly string[]
}

export interface ReleaseInstallerDeps {
  readonly fetch?: typeof globalThis.fetch
  readonly now?: () => number
  readonly onProgress?: (downloadedBytes: number, totalBytes: number) => void
}

/** Reads the commit represented by the currently published installer. */
export async function readReleaseInstallerCommit(
  repositoryUrl: string,
  branch: string,
  token: string | null,
  deps: ReleaseInstallerDeps = {}
): Promise<string> {
  const apiBase = githubApiBase(repositoryUrl)
  if (!apiBase) throw new Error('GitHub installer: поддерживаются только GitHub-репозитории.')
  const request = deps.fetch ?? globalThis.fetch
  const headers = apiHeaders(token)
  const release = await getJson(`${apiBase}/releases/tags/${RELEASE_TAG}`, request, headers)
  const manifestUrl = releaseAssetUrl(release, MANIFEST_ASSET, apiBase)
  if (!manifestUrl) throw new Error('GitHub installer: релиз ещё не содержит манифест.')
  const manifestText = await downloadText(manifestUrl, request, { ...headers, accept: 'application/octet-stream' })
  if (manifestText.length > MAX_MANIFEST_BYTES) throw new Error('GitHub installer: манифест слишком большой.')
  const manifest = parseManifest(manifestText)
  if (manifest.branch !== branch) {
    throw new Error(`GitHub installer: манифест относится к ветке ${manifest.branch}, ожидалась ${branch}.`)
  }
  return manifest.commit
}

interface ReleaseAsset {
  readonly name?: unknown
  readonly url?: unknown
}

/** Downloads only the CI installer that corresponds to the already-green commit. */
export async function downloadReleaseInstaller(
  repositoryUrl: string,
  branch: string,
  commit: string,
  destination: string,
  token: string | null,
  deps: ReleaseInstallerDeps = {},
  onProgress?: (downloadedBytes: number, totalBytes: number) => void
): Promise<DownloadedInstaller> {
  const normalized = normalizeCommit(commit)
  if (!normalized) throw new Error('GitHub installer: некорректный commit.')
  const apiBase = githubApiBase(repositoryUrl)
  if (!apiBase) throw new Error('GitHub installer: поддерживаются только GitHub-репозитории.')

  const request = deps.fetch ?? globalThis.fetch
  const headers = apiHeaders(token)
  const release = await getJson(`${apiBase}/releases/tags/${RELEASE_TAG}`, request, headers)
  const assets: readonly ReleaseAsset[] = Array.isArray(release.assets) ? release.assets : []
  const manifestAsset = assets.find((asset: ReleaseAsset) => asset.name === MANIFEST_ASSET)
  const installerAsset = assets.find((asset: ReleaseAsset) => asset.name === INSTALLER_ASSET)
  const manifestUrl = assetUrl(manifestAsset?.url, apiBase)
  const installerUrl = assetUrl(installerAsset?.url, apiBase)
  if (!manifestAsset || !manifestUrl || !installerAsset || !installerUrl) {
    throw new Error('GitHub installer: релиз ещё не содержит установщик и манифест.')
  }

  const manifestText = await downloadText(manifestUrl, request, { ...headers, accept: 'application/octet-stream' })
  if (manifestText.length > MAX_MANIFEST_BYTES) throw new Error('GitHub installer: манифест слишком большой.')
  const manifest = parseManifest(manifestText)
  if (manifest.commit !== normalized || manifest.branch !== branch) {
    throw new Error(`GitHub installer: манифест относится к ${manifest.commit}, ожидался ${normalized}.`)
  }
  if (manifest.asset !== INSTALLER_ASSET) throw new Error('GitHub installer: неверное имя файла установщика.')

  await mkdir(destination, { recursive: true })
  const installer = join(destination, INSTALLER_ASSET)
  const bytes = await downloadBytes(
    installerUrl,
    installer,
    request,
    { ...headers, accept: 'application/octet-stream' },
    onProgress ?? deps.onProgress,
    manifest.size
  )
  if (bytes !== manifest.size) throw new Error('GitHub installer: размер установщика не совпадает с манифестом.')
  const digest = await sha256(installer)
  if (digest !== manifest.sha256) throw new Error('GitHub installer: SHA-256 установщика не совпадает с манифестом.')

  const marker: BuildMarker = { commit: manifest.commit, branch: manifest.branch, builtAtMs: (deps.now ?? Date.now)() }
  await writeFile(join(destination, 'evohime.build.json'), `${JSON.stringify(marker, null, 2)}\n`, 'utf8')
  return { installer, marker }
}

/** Downloads only selected bounded component artifacts from the same release. */
export async function downloadReleaseComponents(
  repositoryUrl: string,
  commit: string,
  destination: string,
  selected: readonly string[],
  token: string | null,
  deps: ReleaseInstallerDeps = {}
): Promise<DownloadedComponents> {
  const normalized = normalizeCommit(commit)
  const apiBase = githubApiBase(repositoryUrl)
  if (!normalized || !apiBase) throw new Error('GitHub components: некорректный repository или commit.')
  if (selected.length === 0 || selected.length > 32 || selected.some((id) => !/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(id))) {
    throw new Error('GitHub components: некорректный selected component set.')
  }
  const request = deps.fetch ?? globalThis.fetch
  const headers = apiHeaders(token)
  const release = await getJson(`${apiBase}/releases/tags/${RELEASE_TAG}`, request, headers)
  const assets: readonly ReleaseAsset[] = Array.isArray(release.assets) ? release.assets : []
  const manifestAsset = assets.find((asset) => asset.name === COMPONENT_MANIFEST_ASSET)
  const manifestUrl = assetUrl(manifestAsset?.url, apiBase)
  if (!manifestUrl) throw new Error('GitHub components: component manifest отсутствует.')
  const text = await downloadText(manifestUrl, request, { ...headers, accept: 'application/octet-stream' })
  if (text.length > MAX_MANIFEST_BYTES) throw new Error('GitHub components: манифест слишком большой.')
  const manifest = parseComponentManifest(text, normalized)
  const chosen = selected.map((id) => {
    const component = manifest.components.find((candidate) => candidate.id === id)
    if (!component) throw new Error(`GitHub components: компонент не найден: ${id}`)
    return component
  })
  await mkdir(destination, { recursive: true })
  const files: string[] = []
  for (const component of chosen) {
    const asset = assets.find((candidate) => candidate.name === component.artifact)
    const url = assetUrl(asset?.url, apiBase)
    if (!url) throw new Error(`GitHub components: артефакт отсутствует: ${component.artifact}`)
    const target = join(destination, component.path)
    await mkdir(dirname(target), { recursive: true })
    const bytes = await downloadBytes(url, target, request, { ...headers, accept: 'application/octet-stream' }, deps.onProgress, component.size)
    if (bytes !== component.size || (await sha256(target)) !== component.sha256) throw new Error(`GitHub components: hash mismatch: ${component.id}`)
    if (component.id === 'ui-bundle') await extractUiArchive(target, destination)
    files.push(target)
  }
  await writeFile(join(destination, COMPONENT_MANIFEST_ASSET), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
  return { manifest, selected: selected.slice(), files }
}

async function extractUiArchive(archivePath: string, destination: string): Promise<void> {
  const archive = unzipSync(await readFile(archivePath))
  const entries = Object.entries(archive)
  if (entries.length === 0 || entries.length > MAX_UI_FILES) throw new Error('GitHub components: UI archive file count is outside bounds.')
  let total = 0
  for (const [name, bytes] of entries) {
    if (!/^[A-Za-z0-9][A-Za-z0-9._/-]{0,259}$/.test(name) || name.includes('..') || name.includes('//') || name.endsWith('/')) throw new Error('GitHub components: unsafe UI archive path.')
    total += bytes.byteLength
    if (total > MAX_UI_BYTES) throw new Error('GitHub components: UI archive is too large after extraction.')
    const target = join(destination, 'ui-bundle', name)
    await mkdir(dirname(target), { recursive: true })
    await writeFile(target, bytes)
  }
  if (!existsInArchive(entries, 'index.html')) throw new Error('GitHub components: UI archive has no index.html.')
}

function existsInArchive(entries: readonly [string, Uint8Array][], name: string): boolean {
  return entries.some(([entry]) => entry === name || entry === `ui-bundle/${name}`)
}

function releaseAssetUrl(release: any, name: string, apiBase: string): string | null {
  const assets: readonly ReleaseAsset[] = Array.isArray(release?.assets) ? release.assets : []
  const asset = assets.find((candidate: ReleaseAsset) => candidate.name === name)
  return assetUrl(asset?.url, apiBase)
}

function apiHeaders(token: string | null): Record<string, string> {
  const headers: Record<string, string> = {
    accept: 'application/vnd.github+json',
    'user-agent': 'EvoHime-Updater',
    'x-github-api-version': '2022-11-28'
  }
  if (token) headers.authorization = `Bearer ${token}`
  return headers
}

function assetUrl(value: unknown, apiBase: string): string | null {
  if (typeof value !== 'string') return null
  try {
    const candidate = new URL(value)
    const apiOrigin = new URL(apiBase).origin
    return candidate.protocol === 'https:' && candidate.origin === apiOrigin ? candidate.toString() : null
  } catch {
    return null
  }
}

async function getJson(url: string, request: typeof globalThis.fetch, headers: Record<string, string>): Promise<any> {
  const response = await request(url, { headers, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) })
  if (!response.ok) throw new Error(`GitHub installer: API ответил ${response.status}.`)
  return response.json()
}

async function downloadText(url: string, request: typeof globalThis.fetch, headers: Record<string, string>): Promise<string> {
  const response = await request(url, { headers, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) })
  if (!response.ok) throw new Error(`GitHub installer: не удалось скачать манифест (${response.status}).`)
  return response.text()
}

async function downloadBytes(
  url: string,
  path: string,
  request: typeof globalThis.fetch,
  headers: Record<string, string>,
  onProgress?: (downloadedBytes: number, totalBytes: number) => void,
  expectedBytes?: number
): Promise<number> {
  const response = await request(url, { headers, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) })
  if (!response.ok || !response.body) throw new Error(`GitHub installer: не удалось скачать установщик (${response.status}).`)
  const totalBytes = Number(response.headers.get('content-length')) || expectedBytes || 0
  const chunks: Buffer[] = []
  let downloadedBytes = 0
  const reader = response.body.getReader()
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    const chunk = Buffer.from(value)
    chunks.push(chunk)
    downloadedBytes += chunk.byteLength
    onProgress?.(downloadedBytes, totalBytes)
  }
  const data = Buffer.concat(chunks)
  if (data.byteLength > MAX_INSTALLER_BYTES) throw new Error('GitHub installer: установщик слишком большой.')
  await writeFile(path, data)
  return data.byteLength
}

function parseManifest(text: string): ReleaseInstallerManifest {
  let value: any
  try { value = JSON.parse(text) } catch { throw new Error('GitHub installer: повреждённый манифест.') }
  const commit = normalizeCommit(value?.commit)
  const branch = typeof value?.branch === 'string' ? value.branch : null
  const sha256 = typeof value?.sha256 === 'string' && /^[0-9a-f]{64}$/.test(value.sha256) ? value.sha256 : null
  if (!commit || !branch || value?.asset !== INSTALLER_ASSET || !sha256 || !Number.isSafeInteger(value?.size) || value.size <= 0) {
    throw new Error('GitHub installer: некорректный манифест.')
  }
  return { commit, branch, asset: value.asset, sha256, size: value.size }
}

function parseComponentManifest(text: string, commit: string): ReleaseComponentManifest {
  let value: any
  try { value = JSON.parse(text) } catch { throw new Error('GitHub components: повреждённый манифест.') }
  const components = Array.isArray(value?.components) ? value.components : []
  if (value?.schema !== 'evohime.component-manifest.v1' || value?.release_commit !== commit || components.length === 0 || components.length > 32) {
    throw new Error('GitHub components: некорректный манифест.')
  }
  for (const component of components) {
    if (typeof component?.id !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(component.id) || typeof component?.artifact !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(component.artifact) || typeof component?.path !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._/-]{0,259}$/.test(component.path) || component.path.includes('..') || component.path.includes('//') || !Number.isSafeInteger(component.size) || component.size <= 0 || component.size > MAX_INSTALLER_BYTES || typeof component.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(component.sha256)) {
      throw new Error('GitHub components: небезопасная запись компонента.')
    }
  }
  return value as ReleaseComponentManifest
}

async function sha256(path: string): Promise<string> {
  const hash = createHash('sha256')
  hash.update(await readFile(path))
  return hash.digest('hex')
}
