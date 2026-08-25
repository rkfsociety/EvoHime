import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'

import type { CodexModel, CodexRateLimit, CodexRateLimitWindow, CodexStatus } from '@shared/api'

import type { ShellLog } from './diagnostics/logger'
import { describeFailure, runCommand, type CommandRunner } from './update/run-command'

interface PendingRequest {
  readonly resolve: (value: unknown) => void
  readonly reject: (error: Error) => void
}

interface JsonRecord {
  readonly [key: string]: unknown
}

const CLIENT_VERSION = '0.1.0'
const MAX_LINE_CHARS = 512 * 1024
const MAX_MODELS = 100

export type CodexLoginLauncher = (executable: string) => void

export class CodexService {
  private process: ChildProcessWithoutNullStreams | null = null
  private output = ''
  private nextId = 1
  private initialized = false
  private readonly pending = new Map<number, PendingRequest>()
  private status: CodexStatus = emptyStatus()
  private selectedModel: string
  private readonly run: CommandRunner
  private readonly launchLogin: CodexLoginLauncher

  constructor(
    private readonly filePath: string,
    private readonly log: ShellLog,
    private readonly onModelSelected?: (model: string) => Promise<void> | void,
    run?: CommandRunner,
    launchLogin?: CodexLoginLauncher,
  ) {
    this.selectedModel = readSelectedModel(filePath)
    this.run = run ?? runCommand
    this.launchLogin = launchLogin ?? launchCodexLogin
  }

  static defaultPath(dataDirectory: string): string {
    return join(dataDirectory, 'shell', 'codex.json')
  }

  async getStatus(): Promise<CodexStatus> {
    if (this.status.lastUpdatedMs !== null) return this.status
    return this.refresh()
  }

  async refresh(): Promise<CodexStatus> {
    try {
      await this.ensureServer()
      const [modelsResult, limitsResult] = await Promise.all([
        this.request('model/list', { includeHidden: false, limit: MAX_MODELS }),
        this.request('account/rateLimits/read', null)
      ])
      const models = normalizeModels(modelsResult)
      const rateLimits = normalizeRateLimits(limitsResult)
      const selectedModel = chooseModel(this.selectedModel, models)
      if (selectedModel !== this.selectedModel) {
        this.selectedModel = selectedModel
        writeSelectedModel(this.filePath, selectedModel)
      }
      this.status = {
        installed: true,
        installing: false,
        loggingIn: false,
        available: true,
        loggedIn: true,
        selectedModel,
        models,
        rateLimits,
        lastUpdatedMs: Date.now(),
        error: null
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Codex недоступен.'
      this.log('warn', 'shell.codex_status_failed', { reason: safeReason(message) })
      this.status = {
        ...this.status,
        installed: isKnownCodexInstallation(),
        installing: false,
        loggingIn: false,
        available: false,
        loggedIn: false,
        lastUpdatedMs: Date.now(),
        error: message.includes('login') || message.includes('auth')
          ? 'Выполни `codex login`, чтобы подключить аккаунт ChatGPT.'
          : 'Не удалось получить состояние Codex. Проверь установку Codex CLI и вход через ChatGPT.'
      }
    }
    return this.status
  }

  async install(): Promise<CodexStatus> {
    if (this.status.installing) return this.status
    this.status = { ...this.status, installing: true, error: null }
    const result = await this.run({
      file: 'winget',
      args: [
        'install', '--id', 'OpenAI.Codex', '--exact', '--source', 'winget',
        '--silent', '--accept-package-agreements', '--accept-source-agreements',
        '--disable-interactivity'
      ],
      timeoutMs: 45 * 60_000,
      onLine: (line) => this.log('info', 'shell.codex_install_output', { line: line.slice(0, 240) })
    })
    if (result.code !== 0) {
      this.status = {
        ...this.status,
        installing: false,
        lastUpdatedMs: Date.now(),
        error: describeFailure('Установка Codex CLI не выполнена', result)
      }
      return this.status
    }
    this.dispose()
    this.status = emptyStatus()
    return this.refresh()
  }

  async login(): Promise<CodexStatus> {
    if (!isKnownCodexInstallation()) {
      return this.refresh()
    }
    try {
      this.launchLogin(resolveCodexExecutable())
      this.status = {
        ...this.status,
        loggingIn: true,
        lastUpdatedMs: Date.now(),
        error: 'Открыто окно Codex CLI. Заверши вход через ChatGPT и нажми «Обновить».'
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Не удалось открыть вход Codex.'
      this.status = { ...this.status, loggingIn: false, lastUpdatedMs: Date.now(), error: message }
    }
    return this.status
  }

  async selectModel(model: string): Promise<CodexStatus> {
    const normalized = model.trim()
    if (!normalized || normalized.length > 128 || /[\s\0]/.test(normalized)) {
      throw new Error('Некорректная модель Codex.')
    }
    const current = await this.getStatus()
    if (!current.models.some((item) => item.id === normalized || item.model === normalized)) {
      throw new Error('Эта модель не опубликована локальным Codex.')
    }
    this.selectedModel = normalized
    writeSelectedModel(this.filePath, normalized)
    await this.onModelSelected?.(normalized)
    this.status = { ...current, selectedModel: normalized }
    this.log('info', 'shell.codex_model_selected', {})
    return this.status
  }

  dispose(): void {
    this.rejectPending(new Error('Codex завершён.'))
    if (this.process && !this.process.killed) this.process.kill()
    this.process = null
    this.initialized = false
  }

  private async ensureServer(): Promise<void> {
    if (this.process && this.initialized) return
    this.dispose()
    const executable = resolveCodexExecutable()
    const child = spawn(executable, ['app-server', '--stdio'], {
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe']
    })
    this.process = child
    child.stdout.setEncoding('utf8')
    child.stdout.on('data', (chunk: string) => this.consumeOutput(chunk))
    child.stderr.setEncoding('utf8')
    child.stderr.on('data', () => undefined)
    child.on('error', (error) => this.rejectPending(error instanceof Error ? error : new Error('Codex не запустился.')))
    child.on('close', () => {
      this.initialized = false
      if (this.process === child) this.process = null
      this.rejectPending(new Error('Codex app-server завершился.'))
    })
    await this.request('initialize', {
      clientInfo: { name: 'evohime', version: CLIENT_VERSION, title: 'EvoHime' },
      capabilities: { experimentalApi: true }
    })
    this.send({ method: 'initialized', params: {} })
    this.initialized = true
  }

  private request(method: string, params: unknown): Promise<any> {
    if (!this.process) return Promise.reject(new Error('Codex app-server не запущен.'))
    const id = this.nextId++
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      try {
        this.send({ id, method, params })
      } catch (error) {
        this.pending.delete(id)
        reject(error instanceof Error ? error : new Error('Не удалось обратиться к Codex.'))
      }
    })
  }

  private send(message: JsonRecord): void {
    if (!this.process?.stdin.writable) throw new Error('Канал Codex недоступен.')
    this.process.stdin.write(`${JSON.stringify(message)}\n`)
  }

  private consumeOutput(chunk: string): void {
    this.output += chunk
    if (this.output.length > MAX_LINE_CHARS * 2) {
      this.output = this.output.slice(-MAX_LINE_CHARS)
    }
    let boundary = this.output.indexOf('\n')
    while (boundary >= 0) {
      const line = this.output.slice(0, boundary).trim()
      this.output = this.output.slice(boundary + 1)
      if (line.length > 0 && line.length <= MAX_LINE_CHARS) this.acceptMessage(line)
      boundary = this.output.indexOf('\n')
    }
  }

  private acceptMessage(line: string): void {
    let message: JsonRecord
    try {
      const parsed: unknown = JSON.parse(line)
      if (!isRecord(parsed)) return
      message = parsed
    } catch {
      return
    }
    const id = typeof message['id'] === 'number' ? message['id'] : null
    if (id === null) return
    const pending = this.pending.get(id)
    if (!pending) return
    this.pending.delete(id)
    if (isRecord(message['error'])) {
      pending.reject(new Error('Codex отклонил запрос.'))
    } else {
      pending.resolve(message['result'])
    }
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pending.values()) pending.reject(error)
    this.pending.clear()
  }
}

function resolveCodexExecutable(): string {
  const localAppData = process.env['LOCALAPPDATA']?.trim()
  const installed = localAppData ? join(localAppData, 'Programs', 'OpenAI', 'Codex', 'bin', 'codex.exe') : ''
  const npmCodex = process.env['APPDATA']?.trim() ? join(process.env['APPDATA']!, 'npm', 'codex.cmd') : ''
  if (installed && existsSync(installed)) return installed
  if (npmCodex && existsSync(npmCodex)) return npmCodex
  return 'codex'
}

function isKnownCodexInstallation(): boolean {
  return resolveCodexExecutable() !== 'codex'
}

function emptyStatus(): CodexStatus {
  return { installed: isKnownCodexInstallation(), installing: false, loggingIn: false, available: false, loggedIn: false, selectedModel: '', models: [], rateLimits: [], lastUpdatedMs: null, error: null }
}

function launchCodexLogin(executable: string): void {
  // `start` creates a visible interactive terminal; no token or user input is
  // read by EvoHime. The executable path comes only from fixed install paths.
  const child = spawn('cmd.exe', ['/d', '/c', 'start', '"EvoHime Codex login"', executable, 'login'], {
    windowsHide: false,
    stdio: 'ignore',
    detached: true
  })
  child.unref()
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function normalizeModels(value: unknown): CodexModel[] {
  const record = isRecord(value) ? value : {}
  const data = Array.isArray(record['data']) ? record['data'] : []
  return data.flatMap((item): CodexModel[] => {
    if (!isRecord(item) || item['hidden'] === true) return []
    const id = stringValue(item['id'])
    const model = stringValue(item['model']) || id
    if (!id || !model) return []
    const efforts = Array.isArray(item['supportedReasoningEfforts'])
      ? item['supportedReasoningEfforts'].flatMap((entry): string[] => {
        if (typeof entry === 'string') return [entry]
        return isRecord(entry) && typeof entry['reasoningEffort'] === 'string' ? [entry['reasoningEffort']] : []
      }).slice(0, 16)
      : []
    return [{
      id,
      model,
      displayName: stringValue(item['displayName']) || model,
      description: stringValue(item['description']),
      defaultReasoningEffort: stringValue(item['defaultReasoningEffort']),
      supportedReasoningEfforts: efforts,
      isDefault: item['isDefault'] === true
    }]
  }).slice(0, MAX_MODELS)
}

function normalizeRateLimits(value: unknown): CodexRateLimit[] {
  const record = isRecord(value) ? value : {}
  const byId = isRecord(record['rateLimitsByLimitId']) ? record['rateLimitsByLimitId'] : {}
  const entries = Object.entries(byId)
  if (entries.length === 0 && isRecord(record['rateLimits'])) entries.push(['codex', record['rateLimits']])
  return entries.flatMap(([limitId, item]): CodexRateLimit[] => {
    if (!isRecord(item)) return []
    const individual = isRecord(item['individualLimit']) ? item['individualLimit'] : null
    return [{
      limitId,
      planType: stringValue(item['planType']) || null,
      primary: normalizeWindow(item['primary']),
      secondary: normalizeWindow(item['secondary']),
      individualRemainingPercent: integerOrNull(individual?.['remainingPercent']),
      individualResetsAt: integerOrNull(individual?.['resetsAt']),
      reached: item['rateLimitReachedType'] !== null && item['rateLimitReachedType'] !== undefined
    }]
  })
}

function normalizeWindow(value: unknown): CodexRateLimitWindow | null {
  if (!isRecord(value)) return null
  const usedPercent = clampPercent(integerOrNull(value['usedPercent']) ?? 0)
  return {
    usedPercent,
    remainingPercent: 100 - usedPercent,
    resetsAt: integerOrNull(value['resetsAt']),
    windowDurationMins: integerOrNull(value['windowDurationMins'])
  }
}

function chooseModel(selected: string, models: readonly CodexModel[]): string {
  if (selected && models.some((item) => item.id === selected || item.model === selected)) return selected
  return models.find((item) => item.isDefault)?.id ?? models[0]?.id ?? ''
}

function stringValue(value: unknown): string {
  return typeof value === 'string' && value.length <= 4096 ? value : ''
}

function integerOrNull(value: unknown): number | null {
  return typeof value === 'number' && Number.isInteger(value) ? value : null
}

function clampPercent(value: number): number {
  return Math.max(0, Math.min(100, value))
}

function readSelectedModel(filePath: string): string {
  try {
    const parsed: unknown = JSON.parse(readFileSync(filePath, 'utf8'))
    return isRecord(parsed) && typeof parsed['model'] === 'string' && /^[^\s\0]{1,128}$/.test(parsed['model'])
      ? parsed['model']
      : ''
  } catch {
    return ''
  }
}

function writeSelectedModel(filePath: string, model: string): void {
  mkdirSync(dirname(filePath), { recursive: true })
  const temporary = `${filePath}.tmp`
  writeFileSync(temporary, JSON.stringify({ version: 1, model }), { encoding: 'utf8', mode: 0o600 })
  renameSync(temporary, filePath)
}

function safeReason(value: string): string {
  return value.replace(/[\r\n]/g, ' ').slice(0, 160)
}
