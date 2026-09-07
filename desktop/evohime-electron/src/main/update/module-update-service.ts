import { existsSync, readFileSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { join } from 'node:path'

import { disabledUpdateStatus, initialUpdateSteps, type UpdateStatus } from '@shared/update'

export interface ModuleUpdaterStatus {
  readonly schema?: string
  readonly phase?: string
  readonly message?: string
  readonly modules?: readonly string[]
  readonly available?: readonly {
    readonly module?: string
    readonly installed?: string
    readonly available?: string
    readonly summary?: string
    readonly changes?: readonly string[]
  }[]
}

export interface ModuleUpdateServiceOptions {
  readonly dataDirectory: string
  readonly branch: string
  readonly enabled: boolean
  readonly updaterPath: string
  readonly installDirectory: string
  readonly emit: (status: UpdateStatus) => void
  readonly intervalMs: number
}

/**
 * Production-side view of the independent headless Rust updater worker.
 *
 * The Electron shell never checks commits, downloads releases, or applies
 * files. The Rust worker owns those operations and publishes this small
 * status contract for the renderer. The old source-build service remains
 * available only for explicit development runs.
 */
export class ModuleUpdateService {
  private current: UpdateStatus
  private timer: NodeJS.Timeout | null = null
  private lastSerialized = ''

  constructor(private readonly options: ModuleUpdateServiceOptions) {
    this.current = options.enabled
      ? {
          ...disabledUpdateStatus(options.branch),
          phase: 'idle',
          message: 'Состояние модульных обновлений ещё не прочитано.'
        }
      : disabledUpdateStatus(options.branch)
  }

  get status(): UpdateStatus {
    return this.current
  }

  runLaunchGate(): Promise<'continue'> {
    this.refresh()
    if (this.options.enabled && this.timer === null) {
      this.timer = setInterval(() => this.refresh(), this.options.intervalMs)
      this.timer.unref?.()
    }
    return Promise.resolve('continue')
  }

  async check(): Promise<UpdateStatus> {
    this.lastSerialized = ''
    this.patchLocal({ phase: 'checking', message: 'Проверяю версии и целостность модулей…', error: null })
    this.startUpdater('--check')
    return this.current
  }

  async prepare(): Promise<UpdateStatus> {
    return this.check()
  }

  async prepareComponents(_selected: readonly string[]): Promise<UpdateStatus> {
    this.lastSerialized = ''
    this.patchLocal({ phase: 'applying', message: 'Передаю обновление updater worker…' })
    this.startUpdater('--apply')
    return this.current
  }

  restart(): boolean {
    return false
  }

  skip(): UpdateStatus {
    return this.current
  }

  stop(): void {
    if (this.timer !== null) {
      clearInterval(this.timer)
      this.timer = null
    }
  }

  private startUpdater(mode: '--check' | '--apply'): void {
    if (!this.options.enabled) return
    try {
      const child = spawn(
        this.options.updaterPath,
        [mode, '--install-dir', this.options.installDirectory],
        { detached: true, stdio: 'ignore', windowsHide: true }
      )
      child.once('error', () => {
        this.patchLocal({ phase: 'failed', message: 'Не удалось запустить updater worker.', error: 'Updater worker недоступен.' })
      })
      child.unref()
    } catch {
      this.patchLocal({ phase: 'failed', message: 'Не удалось запустить updater worker.' })
    }
  }

  private patchLocal(patch: Partial<UpdateStatus>): void {
    this.current = { ...this.current, ...patch }
    this.options.emit(this.current)
  }

  private refresh(): void {
    if (!this.options.enabled) return
    const path = join(this.options.dataDirectory, 'update-state', 'updater.json')
    if (!existsSync(path)) return
    let parsed: ModuleUpdaterStatus
    try {
      parsed = JSON.parse(readFileSync(path, 'utf8')) as ModuleUpdaterStatus
    } catch {
      return
    }
    const available = (parsed.available ?? []).filter(
      (item): item is Required<Pick<typeof item, 'module' | 'installed' | 'available'>> & typeof item =>
        typeof item.module === 'string' && typeof item.installed === 'string' && typeof item.available === 'string'
    )
    const phase = toUpdatePhase(parsed.phase, available.length > 0)
    const next: UpdateStatus = {
      phase,
      blocking: false,
      message: parsed.message ?? 'Состояние модульных обновлений обновлено.',
      detail: '',
      steps: initialUpdateSteps().map((step) => ({ ...step, state: 'skipped' as const })),
      installedCommit: null,
      installedModules: Object.fromEntries(available.map((item) => [item.module, item.installed])),
      remoteCommit: null,
      branch: this.options.branch,
      error: phase === 'failed' ? parsed.message ?? 'Проверка модулей не удалась.' : null,
      checkedAtMs: Date.now(),
      downloadProgress: null,
      selectedComponents: [],
      availableModules: available.map((item) => item.module),
      availableModuleVersions: Object.fromEntries(available.map((item) => [item.module, item.available])),
      availableModuleSummaries: Object.fromEntries(
        available.map((item) => [item.module, item.summary ?? 'Описание изменений доступно в релизе.'])
      ),
      availableModuleChanges: Object.fromEntries(
        available.map((item) => [item.module, Array.isArray(item.changes) ? item.changes : []])
      ),
      downloadedBytes: null,
      totalBytes: null,
      restartRequired: false,
      evidence: []
    }
    const serialized = JSON.stringify(next)
    if (serialized === this.lastSerialized) return
    this.lastSerialized = serialized
    this.current = next
    this.options.emit(next)
  }
}

function toUpdatePhase(value: string | undefined, hasAvailable: boolean): UpdateStatus['phase'] {
  switch (value) {
    case 'check-failed':
      return 'failed'
    case 'available':
      return 'available'
    case 'checking':
      return 'checking'
    case 'applying':
      return 'applying'
    case 'ready':
      return hasAvailable ? 'available' : 'up-to-date'
    default:
      return hasAvailable ? 'available' : 'up-to-date'
  }
}
