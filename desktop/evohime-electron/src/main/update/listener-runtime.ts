import { createHash } from 'node:crypto'
import { mkdir, readdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import {
  initialListenerRuntimeStatus,
  type ListenerRuntimeStatus
} from '@shared/listener-runtime'

import { githubApiBase } from './commit-status'

/**
 * Набор рантайма распознавания речи: whisper.dll, опциональный ONNX-рантайм и
 * модели лестницы.
 *
 * Доставка идёт тем же механизмом, что и установщик продукта
 * (`release-installer.ts`): ассет постоянного релиза GitHub плюс манифест с
 * SHA-256 каждого файла. Нового хоста и нового корня доверия не заводится.
 *
 * Скачивает только main-процесс. Ни агент, ни Core в сеть за рантаймом не
 * ходят, и ничего не применяется незаметно: обновление предлагается
 * пользователю, а решение принимает он.
 */

const RELEASE_TAG = 'listener-runtime'
const MANIFEST_ASSET = 'listener-runtime.json'
const STAGING_DIRECTORY = '.staging'
const MAX_MANIFEST_BYTES = 64 * 1024
/** Потолок одного файла: самая тяжёлая модель лестницы — около 500 МБ. */
const MAX_FILE_BYTES = 2 * 1024 * 1024 * 1024
/** Потолок всей поставки, чтобы манифест не мог заказать бесконечную загрузку. */
const MAX_TOTAL_BYTES = 4 * 1024 * 1024 * 1024
const REQUEST_TIMEOUT_MS = 120_000
/** Ограниченный backoff: неудача не превращается в цикл запросов к GitHub. */
export const RETRY_BACKOFF_MS = [15_000, 60_000, 300_000] as const

export interface ListenerRuntimeEntry {
  readonly role: string
  readonly name: string
  readonly sha256: string
  readonly size: number
}

export interface ListenerRuntimeModel {
  readonly rung: string
  readonly name: string
  readonly sha256: string
  readonly size: number
}

export interface ListenerRuntimeManifest {
  readonly schema: number
  readonly version: string
  readonly abi: { readonly name: string; readonly context_params_size: number; readonly full_params_size: number }
  readonly files: readonly ListenerRuntimeEntry[]
  readonly models: readonly ListenerRuntimeModel[]
}

export interface ListenerRuntimeDeps {
  readonly toolsDirectory: string
  readonly repositoryUrl: string
  readonly resolveToken: () => Promise<string | null>
  readonly emit: (status: ListenerRuntimeStatus) => void
  readonly log: (level: 'info' | 'warn' | 'error', event: string, fields: Record<string, unknown>) => void
  readonly fetch?: typeof globalThis.fetch
  readonly now?: () => number
}

export class ListenerRuntimeService {
  private current: ListenerRuntimeStatus
  private running = false
  private failures = 0
  private nextAttemptAtMs = 0

  constructor(private readonly deps: ListenerRuntimeDeps) {
    this.current = initialListenerRuntimeStatus(deps.toolsDirectory)
  }

  get status(): ListenerRuntimeStatus {
    return this.current
  }

  /**
   * Сверяет установленный набор с опубликованным.
   *
   * Сетевая часть необязательна: если GitHub недоступен, установленный
   * рантайм всё равно остаётся рабочим, и об этом говорится прямо, а не
   * подменяется словом «ошибка».
   */
  async check(): Promise<ListenerRuntimeStatus> {
    const installed = await this.readInstalledManifest()
    let available: ListenerRuntimeManifest | null = null
    let networkError: string | null = null
    try {
      available = await this.readPublishedManifest()
    } catch (error) {
      networkError = error instanceof Error ? error.message : String(error)
    }

    if (!installed) {
      return this.patch({
        state: 'missing',
        installedVersion: null,
        availableVersion: available?.version ?? null,
        missingOptional: [],
        message: available
          ? `Распознавание речи не установлено. Доступна версия ${available.version}.`
          : `Распознавание речи не установлено. Проверить обновление не удалось: ${networkError ?? 'нет данных'}.`
      })
    }

    const missingOptional = await this.missingOptionalFiles(installed)
    if (available && available.version !== installed.version) {
      return this.patch({
        state: 'update-available',
        installedVersion: installed.version,
        availableVersion: available.version,
        missingOptional,
        message: `Установлена версия ${installed.version}, доступна ${available.version}.`
      })
    }
    return this.patch({
      state: 'ready',
      installedVersion: installed.version,
      availableVersion: available?.version ?? null,
      missingOptional,
      message: networkError
        ? `Установлена версия ${installed.version}. Проверить обновление не удалось.`
        : `Установлена версия ${installed.version}.`
    })
  }

  /**
   * Скачивает и переключает набор.
   *
   * Загрузка идёт в staging-каталог, манифест переименовывается последним:
   * до этого момента листенер продолжает видеть прежний рабочий набор, а
   * прерванная загрузка не оставляет полуустановленного рантайма.
   */
  async download(): Promise<ListenerRuntimeStatus> {
    if (this.running) return this.current
    const now = (this.deps.now ?? Date.now)()
    if (now < this.nextAttemptAtMs) {
      return this.patch({
        state: this.current.state === 'downloading' ? 'failed' : this.current.state,
        message: `Повторная попытка будет доступна через ${Math.ceil((this.nextAttemptAtMs - now) / 1000)} с.`
      })
    }
    this.running = true
    try {
      const manifest = await this.readPublishedManifest()
      this.patch({
        state: 'downloading',
        availableVersion: manifest.version,
        progressPct: 0,
        message: `Загрузка распознавания речи ${manifest.version}…`
      })
      const staging = join(this.deps.toolsDirectory, STAGING_DIRECTORY)
      await rm(staging, { recursive: true, force: true })
      await mkdir(staging, { recursive: true })

      const entries = [...manifest.files, ...manifest.models]
      const total = entries.reduce((sum, entry) => sum + entry.size, 0)
      if (total > MAX_TOTAL_BYTES) throw new Error('Манифест рантайма запрашивает слишком много данных.')
      let done = 0
      const token = await this.deps.resolveToken()
      const release = await this.getRelease(token)
      for (const entry of entries) {
        const url = assetUrl(release, assetNameOf(entry.name), githubApiBase(this.deps.repositoryUrl))
        if (!url) throw new Error(`В релизе нет файла ${entry.name}.`)
        const target = containedPath(staging, entry.name)
        await mkdir(join(target, '..'), { recursive: true })
        const bytes = await this.downloadFile(url, target, token, entry.size, (chunk) => {
          done += chunk
          this.patch({
            progressPct: total > 0 ? Math.min(99, Math.floor((done / total) * 100)) : 0
          })
        })
        if (bytes !== entry.size) throw new Error(`Размер ${entry.name} не совпадает с манифестом.`)
        const digest = await sha256(target)
        if (digest !== entry.sha256) throw new Error(`SHA-256 ${entry.name} не совпадает с манифестом.`)
      }

      await this.activate(staging, manifest)
      this.failures = 0
      this.nextAttemptAtMs = 0
      this.deps.log('info', 'listener_runtime.installed', { version: manifest.version })
      return this.patch({ ...(await this.check()), progressPct: 100 })
    } catch (error) {
      this.failures += 1
      const backoff =
        RETRY_BACKOFF_MS[Math.min(this.failures - 1, RETRY_BACKOFF_MS.length - 1)] ??
        RETRY_BACKOFF_MS[RETRY_BACKOFF_MS.length - 1]!
      this.nextAttemptAtMs = (this.deps.now ?? Date.now)() + backoff
      const message = error instanceof Error ? error.message : String(error)
      this.deps.log('warn', 'listener_runtime.download_failed', { message })
      return this.patch({
        state: 'failed',
        progressPct: 0,
        message: `Не удалось установить распознавание речи: ${message}`
      })
    } finally {
      this.running = false
    }
  }

  /**
   * Переносит проверенные файлы в рабочий каталог.
   *
   * Манифест пишется последним и уже после файлов: пока его нет, листенер
   * считает каталог непригодным и не пытается загрузить половину поставки.
   */
  private async activate(staging: string, manifest: ListenerRuntimeManifest): Promise<void> {
    const manifestPath = join(this.deps.toolsDirectory, MANIFEST_ASSET)
    await rm(manifestPath, { force: true })
    for (const entry of [...manifest.files, ...manifest.models]) {
      const from = containedPath(staging, entry.name)
      const to = containedPath(this.deps.toolsDirectory, entry.name)
      await mkdir(join(to, '..'), { recursive: true })
      // Старый файл может быть открыт листенером через mmap: Windows тогда
      // не даст его удалить, и это единственная надёжная проверка «модель
      // ещё используется». Ошибку не глотаем — переключение не состоялось.
      await rm(to, { force: true })
      await rename(from, to)
    }
    const staged = join(staging, MANIFEST_ASSET)
    await writeFile(staged, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
    await rename(staged, manifestPath)
    await this.removeUnusedFiles(manifest)
    await rm(staging, { recursive: true, force: true })
  }

  /** Удаляет файлы прежних версий — только после успешного переключения. */
  private async removeUnusedFiles(manifest: ListenerRuntimeManifest): Promise<void> {
    const keep = new Set<string>([
      MANIFEST_ASSET,
      ...[...manifest.files, ...manifest.models].map((entry) => entry.name.replace(/\\/g, '/'))
    ])
    const walk = async (relative: string): Promise<void> => {
      const absolute = relative ? join(this.deps.toolsDirectory, relative) : this.deps.toolsDirectory
      for (const item of await readdir(absolute, { withFileTypes: true })) {
        if (item.name === STAGING_DIRECTORY) continue
        const child = relative ? `${relative}/${item.name}` : item.name
        if (item.isDirectory()) {
          await walk(child)
          continue
        }
        if (keep.has(child)) continue
        // Занятый файл оставляем на месте: он ещё отображён в память
        // работающего листенера, а тихо ронять переключение из-за уборки
        // нельзя.
        await rm(join(this.deps.toolsDirectory, child), { force: true }).catch(() => {})
      }
    }
    await walk('')
  }

  private async readInstalledManifest(): Promise<ListenerRuntimeManifest | null> {
    try {
      const path = join(this.deps.toolsDirectory, MANIFEST_ASSET)
      const info = await stat(path)
      if (info.size > MAX_MANIFEST_BYTES) return null
      return parseManifest(await readFile(path, 'utf8'))
    } catch {
      return null
    }
  }

  private async missingOptionalFiles(manifest: ListenerRuntimeManifest): Promise<string[]> {
    const optional = manifest.files.filter((entry) => entry.role === 'onnxruntime_dll' || entry.role === 'silero_vad')
    const missing: string[] = []
    for (const entry of optional) {
      try {
        await stat(containedPath(this.deps.toolsDirectory, entry.name))
      } catch {
        missing.push(entry.role)
      }
    }
    return missing
  }

  private async readPublishedManifest(): Promise<ListenerRuntimeManifest> {
    const token = await this.deps.resolveToken()
    const release = await this.getRelease(token)
    const url = assetUrl(release, MANIFEST_ASSET, githubApiBase(this.deps.repositoryUrl))
    if (!url) throw new Error('Релиз рантайма ещё не содержит манифест.')
    const text = await this.downloadText(url, token)
    if (text.length > MAX_MANIFEST_BYTES) throw new Error('Манифест рантайма слишком большой.')
    return parseManifest(text)
  }

  private async getRelease(token: string | null): Promise<unknown> {
    const apiBase = githubApiBase(this.deps.repositoryUrl)
    if (!apiBase) throw new Error('Рантайм распознавания поставляется только через GitHub-релизы.')
    const request = this.deps.fetch ?? globalThis.fetch
    const response = await request(`${apiBase}/releases/tags/${RELEASE_TAG}`, {
      headers: apiHeaders(token),
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
    })
    if (!response.ok) throw new Error(`GitHub ответил ${response.status}.`)
    return response.json()
  }

  private async downloadText(url: string, token: string | null): Promise<string> {
    const request = this.deps.fetch ?? globalThis.fetch
    const response = await request(url, {
      headers: { ...apiHeaders(token), accept: 'application/octet-stream' },
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
    })
    if (!response.ok) throw new Error(`не удалось скачать манифест (${response.status})`)
    return response.text()
  }

  private async downloadFile(
    url: string,
    target: string,
    token: string | null,
    expectedBytes: number,
    onChunk: (bytes: number) => void
  ): Promise<number> {
    if (expectedBytes <= 0 || expectedBytes > MAX_FILE_BYTES) {
      throw new Error('Манифест объявляет недопустимый размер файла.')
    }
    const request = this.deps.fetch ?? globalThis.fetch
    const response = await request(url, {
      headers: { ...apiHeaders(token), accept: 'application/octet-stream' },
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
    })
    if (!response.ok || !response.body) throw new Error(`не удалось скачать файл (${response.status})`)
    const chunks: Buffer[] = []
    let downloaded = 0
    const reader = response.body.getReader()
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      const chunk = Buffer.from(value)
      downloaded += chunk.byteLength
      if (downloaded > expectedBytes) throw new Error('Файл больше объявленного в манифесте размера.')
      chunks.push(chunk)
      onChunk(chunk.byteLength)
    }
    await writeFile(target, Buffer.concat(chunks))
    return downloaded
  }

  private patch(patch: Partial<ListenerRuntimeStatus>): ListenerRuntimeStatus {
    this.current = { ...this.current, ...patch }
    this.deps.emit(this.current)
    return this.current
  }
}

/** Ассет релиза называется по базовому имени: подкаталогов у ассетов нет. */
export function assetNameOf(name: string): string {
  const parts = name.split(/[\\/]/)
  return parts[parts.length - 1] ?? name
}

/**
 * Путь строго внутри каталога.
 *
 * Манифест приходит по сети, поэтому `..` в имени — это не теоретическая
 * возможность, а первое, что стоит проверить: без этой проверки загрузка
 * писала бы файлы куда угодно на диске.
 */
export function containedPath(root: string, name: string): string {
  const parts = name.split(/[\\/]/).filter((part) => part.length > 0)
  if (parts.length === 0 || parts.some((part) => part === '.' || part === '..' || /^[a-zA-Z]:$/.test(part))) {
    throw new Error(`Манифест указывает путь за пределы каталога: ${name}`)
  }
  return join(root, ...parts)
}

export function parseManifest(text: string): ListenerRuntimeManifest {
  let value: any
  try {
    value = JSON.parse(text)
  } catch {
    throw new Error('Повреждённый манифест рантайма.')
  }
  const version = typeof value?.version === 'string' ? value.version : ''
  const abi = value?.abi
  if (
    value?.schema !== 1 ||
    !/^[A-Za-z0-9._:+-]{1,128}$/.test(version) ||
    typeof abi?.name !== 'string' ||
    !Number.isSafeInteger(abi?.context_params_size) ||
    !Number.isSafeInteger(abi?.full_params_size) ||
    !Array.isArray(value?.files) ||
    !Array.isArray(value?.models) ||
    value.models.length === 0
  ) {
    throw new Error('Некорректный манифест рантайма.')
  }
  const files = value.files.map((entry: unknown) => parseEntry(entry, 'role'))
  const models = value.models.map((entry: unknown) => parseEntry(entry, 'rung'))
  return { schema: 1, version, abi, files, models }
}

function parseEntry(entry: any, kindField: 'role' | 'rung'): any {
  const name = typeof entry?.name === 'string' ? entry.name : ''
  const sha256Value = typeof entry?.sha256 === 'string' && /^[0-9a-f]{64}$/.test(entry.sha256) ? entry.sha256 : null
  if (!name || !sha256Value || typeof entry?.[kindField] !== 'string' || !Number.isSafeInteger(entry?.size) || entry.size <= 0) {
    throw new Error('Некорректная запись в манифесте рантайма.')
  }
  return { [kindField]: entry[kindField], name, sha256: sha256Value, size: entry.size }
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

function assetUrl(release: any, name: string, apiBase: string | null): string | null {
  if (!apiBase) return null
  const assets: readonly { readonly name?: unknown; readonly url?: unknown }[] = Array.isArray(release?.assets)
    ? release.assets
    : []
  const asset = assets.find((candidate) => candidate.name === name)
  if (typeof asset?.url !== 'string') return null
  try {
    const candidate = new URL(asset.url)
    return candidate.protocol === 'https:' && candidate.origin === new URL(apiBase).origin
      ? candidate.toString()
      : null
  } catch {
    return null
  }
}

async function sha256(path: string): Promise<string> {
  const hash = createHash('sha256')
  hash.update(await readFile(path))
  return hash.digest('hex')
}
