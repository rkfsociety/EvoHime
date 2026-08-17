import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { normalizeCommit, type BuildMarker } from './config'
import { githubApiBase } from './commit-status'

const RELEASE_TAG = 'installer'
const INSTALLER_ASSET = 'EvoHime-Setup.exe'
const MANIFEST_ASSET = 'EvoHime-Setup.json'
const MAX_MANIFEST_BYTES = 64 * 1024
const MAX_INSTALLER_BYTES = 2 * 1024 * 1024 * 1024
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

export interface ReleaseInstallerDeps {
  readonly fetch?: typeof globalThis.fetch
  readonly now?: () => number
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
  deps: ReleaseInstallerDeps = {}
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
  const bytes = await downloadBytes(installerUrl, installer, request, { ...headers, accept: 'application/octet-stream' })
  if (bytes !== manifest.size) throw new Error('GitHub installer: размер установщика не совпадает с манифестом.')
  const digest = await sha256(installer)
  if (digest !== manifest.sha256) throw new Error('GitHub installer: SHA-256 установщика не совпадает с манифестом.')

  const marker: BuildMarker = { commit: manifest.commit, branch: manifest.branch, builtAtMs: (deps.now ?? Date.now)() }
  await writeFile(join(destination, 'evohime.build.json'), `${JSON.stringify(marker, null, 2)}\n`, 'utf8')
  return { installer, marker }
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

async function downloadBytes(url: string, path: string, request: typeof globalThis.fetch, headers: Record<string, string>): Promise<number> {
  const response = await request(url, { headers, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) })
  if (!response.ok || !response.body) throw new Error(`GitHub installer: не удалось скачать установщик (${response.status}).`)
  const data = Buffer.from(await response.arrayBuffer())
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

async function sha256(path: string): Promise<string> {
  const hash = createHash('sha256')
  hash.update(await readFile(path))
  return hash.digest('hex')
}
