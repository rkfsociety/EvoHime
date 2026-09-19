import { existsSync, readFileSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { join } from 'node:path'

import { disabledUpdateStatus, initialUpdateSteps, type UpdateStatus } from '@shared/update'

export interface ModuleUpdaterStatus {
  readonly schema?: string
  readonly phase?: string
  readonly message?: string
  readonly error?: string | null
  readonly modules?: readonly string[]
  readonly available?: readonly {
    readonly module?: string
    readonly installed?: string
    readonly available?: string
    readonly summary?: string
    readonly changes?: readonly string[]
  }[]
  readonly requires_exit?: boolean
  readonly recovery?: UpdateStatus['recovery']
}

export interface ModuleUpdateServiceOptions {
  readonly dataDirectory: string
  readonly branch: string
  readonly enabled: boolean
  readonly updaterPath: string
  readonly installDirectory: string
  readonly emit: (status: UpdateStatus) => void
  readonly intervalMs: number
  /** Skip only the first launch check when a transaction has just relaunched the shell. */
  readonly skipLaunchGate?: boolean
  /** Closes the visible updater only after downloads finish and file replacement must begin. */
  readonly quitForApply?: () => void
}

/** The native updater worker is installed beside the packaged shell binary. */
export function resolveInstalledUpdaterPath(installDirectory: string): string {
  return join(installDirectory, 'evohime-updater.exe')
}

/** A bootstrap install has no shell to launch until its first module apply. */
export function shouldApplyBootstrap(shellExists: boolean, availableModules?: readonly string[]): boolean {
  return !shellExists && Boolean(availableModules?.length)
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
  private exitRequested = false

  constructor(private readonly options: ModuleUpdateServiceOptions) {
    this.current = options.enabled
      ? {
          ...disabledUpdateStatus(options.branch),
          phase: 'idle',
          message: 'Состояние модульных обновлений ещё не прочитано.',
          installedModules: readInstalledModules(options.installDirectory)
        }
      : {
          ...disabledUpdateStatus(options.branch),
          installedModules: readInstalledModules(options.installDirectory)
        }
  }

  get status(): UpdateStatus {
    return this.current
  }

  async runLaunchGate(): Promise<'continue' | 'applying'> {
    if (!this.options.enabled) {
      this.scheduleRefresh()
      return 'continue'
    }

    // The transaction worker starts the new shell before it can commit its
    // state. Running the normal gate here would observe that still-open
    // transaction, launch another updater, and prevent the health handshake.
    // Resume periodic checks after the shell has authenticated with Core.
    if (this.options.skipLaunchGate) {
      this.scheduleRefresh()
      return 'continue'
    }

    this.patchLocal({ blocking: true })
    const checked = await this.check()
    if (checked.phase === 'failed') return this.releaseGate()

    const availableModules = checked.availableModules ?? []
    if (availableModules.length === 0) return this.releaseGate()

    // A module update is part of launching EvoHime. There is deliberately no
    // user choice here: the regular shell remains closed until the detached
    // worker has taken ownership of the apply transaction.
    await this.prepareComponents(availableModules)
    return 'applying'
  }

  async check(): Promise<UpdateStatus> {
    this.lastSerialized = ''
    this.patchLocal({ phase: 'checking', message: 'Проверяю версии и целостность модулей…', error: null })
    await this.startUpdater('--check')
    return this.current
  }

  async prepare(): Promise<UpdateStatus> {
    return this.check()
  }

  async prepareComponents(_selected: readonly string[]): Promise<UpdateStatus> {
    this.lastSerialized = ''
    this.exitRequested = false
    this.patchLocal({ phase: 'applying', message: 'Передаю обновление updater worker…' })
    this.scheduleRefresh()
    void this.startUpdater('--apply')
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

  private scheduleRefresh(): void {
    if (this.options.enabled && this.timer === null) {
      this.timer = setInterval(() => this.refresh(), this.options.intervalMs)
      this.timer.unref?.()
    }
  }

  private releaseGate(): 'continue' {
    this.patchLocal({ blocking: false })
    this.scheduleRefresh()
    return 'continue'
  }

  private startUpdater(mode: '--check' | '--apply'): Promise<boolean> {
    if (!this.options.enabled) return Promise.resolve(false)
    const args = [
      mode,
      '--install-dir',
      this.options.installDirectory
    ]
    if (mode === '--apply') {
      args.push(
        '--wait-pid', String(process.pid),
        '--relaunch', join(this.options.installDirectory, 'EvoHime.exe'),
        '--health-file', join(this.options.dataDirectory, 'update-state', 'health.json')
      )
    }
    let resolveCompletion: (succeeded: boolean) => void = () => {}
    const completion = new Promise<boolean>((resolve) => { resolveCompletion = resolve })
    try {
      const child = spawn(
        this.options.updaterPath,
        args,
        { detached: true, stdio: 'ignore', windowsHide: true, shell: false }
      )
      child.once('error', () => {
        this.patchLocal({ phase: 'failed', message: 'Не удалось запустить updater worker.', error: 'Updater worker недоступен.' })
        resolveCompletion(false)
      })
      child.once('close', (code: number | null, signal: NodeJS.Signals | null) => {
        this.refresh()
        if (code === 0 && !['checking', 'applying'].includes(this.current.phase)) {
          resolveCompletion(true)
          return
        }
        // A correctly written Rust status is richer and is picked up above by
        // refresh(). This fallback still makes a worker crash or an unwritable
        // status file visible instead of leaving the UI in "applying" forever.
        const reason = code === null
          ? `сигналом ${signal ?? 'неизвестным'}`
          : code === 0 ? 'успешно, но без диагностического статуса' : `кодом ${code}`
        const action = mode === '--apply' ? 'применить' : 'проверить'
        this.patchLocal({
          phase: 'failed',
          message: `Не удалось ${action} модульные обновления.`,
          error: code === 0
            ? `updater: worker завершился ${reason}.`
            : `updater: worker завершился с ${reason} без диагностического статуса.`
        })
        resolveCompletion(false)
      })
      child.unref()
      return completion
    } catch {
      this.patchLocal({ phase: 'failed', message: 'Не удалось запустить updater worker.' })
      resolveCompletion(false)
      return completion
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
    const installedModules = {
      ...readInstalledModules(this.options.installDirectory),
      ...Object.fromEntries(available.map((item) => [item.module, item.installed]))
    }
    const phase = toUpdatePhase(parsed.phase, available.length > 0)
    const next: UpdateStatus = {
      phase,
      blocking: false,
      message: parsed.message ?? 'Состояние модульных обновлений обновлено.',
      detail: '',
      steps: initialUpdateSteps().map((step) => ({ ...step, state: 'skipped' as const })),
      installedCommit: null,
      installedModules,
      remoteCommit: null,
      branch: this.options.branch,
      error: phase === 'failed' ? parsed.error ?? parsed.message ?? 'Проверка модулей не удалась.' : null,
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
      evidence: [],
      ...(parsed.recovery ? { recovery: parsed.recovery } : {})
    }
    const serialized = JSON.stringify(next)
    if (serialized === this.lastSerialized) return
    this.lastSerialized = serialized
    this.current = next
    this.options.emit(next)
    if (parsed.requires_exit === true && !this.exitRequested) {
      this.exitRequested = true
      this.options.quitForApply?.()
    }
  }
}

function readInstalledModules(installDirectory: string): Readonly<Record<string, string>> {
  try {
    const value = JSON.parse(readFileSync(join(installDirectory, 'evohime.components.json'), 'utf8')) as {
      components?: Array<{ id?: unknown; version?: unknown }>
    }
    const result: Record<string, string> = {}
    for (const component of value.components ?? []) {
      if (typeof component.id === 'string' && typeof component.version === 'string') {
        result[component.id] = component.version
      }
    }
    return result
  } catch {
    return {}
  }
}

function toUpdatePhase(value: string | undefined, hasAvailable: boolean): UpdateStatus['phase'] {
  switch (value) {
    case 'failed':
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
