import { randomUUID } from 'node:crypto'
import { access, mkdtemp, open, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { spawn } from 'node:child_process'

import {
  initialOllamaRuntimeStatus,
  type OllamaRuntimeStatus
} from '@shared/ollama-runtime'

export const OLLAMA_INSTALLER_URL = 'https://ollama.com/download/OllamaSetup.exe'
export const OLLAMA_API_URL = 'http://127.0.0.1:11434/api/version'

const MAX_INSTALLER_BYTES = 2 * 1024 * 1024 * 1024
const MAX_VERSION_RESPONSE_BYTES = 16 * 1024
const REQUEST_TIMEOUT_MS = 15 * 60 * 1000
const PROBE_TIMEOUT_MS = 2_500
const PROBE_ATTEMPTS = 30
const PROBE_INTERVAL_MS = 1_000
const ALLOWED_INSTALLER_HOSTS = new Set([
  'release-assets.githubusercontent.com',
  'objects.githubusercontent.com'
])

type FetchLike = typeof globalThis.fetch

export interface OllamaRuntimeDeps {
  readonly fetch?: FetchLike
  readonly environment?: NodeJS.ProcessEnv
  readonly emit: (status: OllamaRuntimeStatus) => void
  readonly log: (level: 'info' | 'warn' | 'error', event: string, fields: Record<string, unknown>) => void
  readonly exists?: (path: string) => Promise<boolean>
  readonly launchInstaller?: (path: string) => Promise<number | null>
  readonly wait?: (milliseconds: number) => Promise<void>
}

/** Main-process owner of Ollama discovery, installation and readiness checks. */
export class OllamaRuntimeService {
  private current = initialOllamaRuntimeStatus()
  private installing = false

  constructor(private readonly deps: OllamaRuntimeDeps) {}

  get status(): OllamaRuntimeStatus {
    return this.current
  }

  async check(): Promise<OllamaRuntimeStatus> {
    const executablePath = await this.findExecutable()
    const probe = await this.probe()
    if (probe.available) {
      return this.patch({
        state: 'ready',
        version: probe.version,
        executablePath,
        downloadedBytes: 0,
        totalBytes: null,
        message: probe.version ? `Ollama готова (версия ${probe.version}).` : 'Ollama готова.'
      })
    }
    if (executablePath) {
      return this.patch({
        state: 'installed',
        version: null,
        executablePath,
        downloadedBytes: 0,
        totalBytes: null,
        message: 'Ollama установлена, но локальный API пока недоступен. Запусти Ollama и проверь снова.'
      })
    }
    return this.patch({
      state: 'missing',
      version: null,
      executablePath: null,
      downloadedBytes: 0,
      totalBytes: null,
      message: 'Ollama не установлена. Её можно установить одной кнопкой ниже.'
    })
  }

  async install(): Promise<OllamaRuntimeStatus> {
    if (this.installing) return this.current
    this.installing = true
    let staging = ''
    try {
      // The UI hides the install button once discovery succeeds, but this
      // guard also protects the main-process command from stale or forged UI
      // state and prevents an unnecessary reinstall.
      const existing = await this.check()
      if (existing.state === 'ready' || existing.state === 'installed') return existing
      staging = await mkdtemp(join(tmpdir(), 'evohime-ollama-'))
      const target = join(staging, `OllamaSetup-${randomUUID()}.exe`)
      this.patch({
        state: 'installing',
        downloadedBytes: 0,
        totalBytes: null,
        message: 'Скачиваем официальный установщик Ollama…'
      })
      await this.downloadInstaller(target)
      this.patch({ message: 'Запускаем установщик Ollama…' })
      const exitCode = await (this.deps.launchInstaller ?? launchInstaller)(target)
      if (exitCode !== 0) {
        throw new Error(exitCode === null ? 'установщик был закрыт' : `установщик завершился с кодом ${exitCode}`)
      }

      this.patch({ message: 'Проверяем запущенный локальный API Ollama…' })
      for (let attempt = 0; attempt < PROBE_ATTEMPTS; attempt += 1) {
        const status = await this.check()
        if (status.state === 'ready') {
          this.deps.log('info', 'shell.ollama_installed', { version: status.version ?? '' })
          return status
        }
        await (this.deps.wait ?? delay)(PROBE_INTERVAL_MS)
      }
      const status = await this.check()
      if (status.state === 'installed') {
        return this.patch({ message: 'Ollama установлена. Запусти её из меню Пуск и нажми «Проверить».' })
      }
      throw new Error('Ollama установлена, но её локальный API не отвечает')
    } catch (error) {
      const message = describeOllamaError(error)
      this.deps.log('warn', 'shell.ollama_install_failed', { message })
      return this.patch({
        state: 'failed',
        downloadedBytes: 0,
        totalBytes: null,
        message: `Не удалось установить Ollama: ${message}`
      })
    } finally {
      this.installing = false
      if (staging) await rm(staging, { recursive: true, force: true }).catch(() => {})
    }
  }

  private async downloadInstaller(target: string): Promise<void> {
    const request = this.deps.fetch ?? globalThis.fetch
    const response = await request(OLLAMA_INSTALLER_URL, {
      redirect: 'follow',
      headers: { accept: 'application/octet-stream', 'user-agent': 'EvoHime' },
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
    })
    if (!response.ok || !response.body) throw new Error(`сервер Ollama ответил ${response.status}`)
    if (!isAllowedInstallerUrl(response.url || OLLAMA_INSTALLER_URL)) {
      throw new Error('загрузка перенаправлена на неподдерживаемый адрес')
    }
    const contentLength = Number(response.headers.get('content-length') ?? 0)
    if (contentLength > MAX_INSTALLER_BYTES) throw new Error('установщик Ollama слишком большой')

    const file = await open(target, 'wx')
    let downloadedBytes = 0
    const header: number[] = []
    try {
      const reader = response.body.getReader()
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        downloadedBytes += value.byteLength
        if (downloadedBytes > MAX_INSTALLER_BYTES) throw new Error('установщик Ollama превысил допустимый размер')
        for (const byte of value) {
          if (header.length >= 2) break
          header.push(byte)
        }
        await file.write(value)
        this.patch({ downloadedBytes, totalBytes: contentLength > 0 ? contentLength : null })
      }
    } finally {
      await file.close()
    }
    if (downloadedBytes === 0) throw new Error('сервер Ollama вернул пустой установщик')
    if (header[0] !== 0x4d || header[1] !== 0x5a) throw new Error('официальный адрес вернул не Windows-инсталлятор')
  }

  private async findExecutable(): Promise<string | null> {
    const environment = this.deps.environment ?? process.env
    const localAppData = environment.LOCALAPPDATA?.trim()
    const candidates = [
      localAppData ? join(localAppData, 'Programs', 'Ollama', 'ollama.exe') : '',
      environment.ProgramW6432 ? join(environment.ProgramW6432, 'Ollama', 'ollama.exe') : '',
      environment.ProgramFiles ? join(environment.ProgramFiles, 'Ollama', 'ollama.exe') : '',
      ...(environment.Path ?? environment.PATH ?? '').split(delimiter).map((directory) => directory ? join(directory, 'ollama.exe') : '')
    ].filter(Boolean)
    const exists = this.deps.exists ?? fileExists
    for (const candidate of [...new Set(candidates)]) {
      if (await exists(candidate)) return candidate
    }
    return null
  }

  private async probe(): Promise<{ available: boolean; version: string | null }> {
    const request = this.deps.fetch ?? globalThis.fetch
    try {
      const response = await request(OLLAMA_API_URL, { signal: AbortSignal.timeout(PROBE_TIMEOUT_MS) })
      if (!response.ok) return { available: false, version: null }
      const body = await response.text()
      if (Buffer.byteLength(body, 'utf8') > MAX_VERSION_RESPONSE_BYTES) return { available: false, version: null }
      const value = JSON.parse(body) as { version?: unknown }
      const version = typeof value.version === 'string' && value.version.length <= 128 ? value.version : null
      return { available: true, version }
    } catch {
      return { available: false, version: null }
    }
  }

  private patch(patch: Partial<OllamaRuntimeStatus>): OllamaRuntimeStatus {
    this.current = { ...this.current, ...patch }
    this.deps.emit(this.current)
    return this.current
  }
}

export function isAllowedInstallerUrl(value: string): boolean {
  try {
    const url = new URL(value)
    if (url.protocol !== 'https:') return false
    if (url.hostname === 'ollama.com') return url.pathname === '/download/OllamaSetup.exe'
    if (url.hostname === 'github.com') return url.pathname.startsWith('/ollama/ollama/releases/')
    return ALLOWED_INSTALLER_HOSTS.has(url.hostname)
  } catch {
    return false
  }
}

function launchInstaller(path: string): Promise<number | null> {
  return new Promise((resolve, reject) => {
    const child = spawn(path, [], { windowsHide: true, stdio: 'ignore' })
    child.once('error', reject)
    child.once('exit', (code) => resolve(code))
  })
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await access(path)
    return true
  } catch {
    return false
  }
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds))
}

function describeOllamaError(error: unknown): string {
  if (!(error instanceof Error)) return String(error)
  const message = error.message.trim()
  if (message !== 'fetch failed') return message || 'неизвестная ошибка'
  const cause = error.cause
  return cause instanceof Error && cause.message.trim() ? `сетевая ошибка: ${cause.message.trim()}` : 'сетевая ошибка'
}
