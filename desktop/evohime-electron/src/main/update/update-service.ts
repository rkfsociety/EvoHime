import { existsSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { join } from 'node:path'

import {
  disabledUpdateStatus,
  initialUpdateSteps,
  type UpdateStatus,
  type UpdateStep,
  type UpdateStepId,
  type UpdateStepState
} from '@shared/update'

import { redactError } from '../diagnostics/redact'
import type { BuildLogWriter } from './build-log'
import { buildStagedPackage, clearDerivedState } from './builder'
import { readBuildMarker, type UpdateConfig } from './config'
import { readRemoteHead, syncCheckout } from './source-checkout'
import { detectToolchain, ensureToolchain, toolPath, type ToolchainReport } from './toolchain'

/**
 * Orchestrates the source-based update: check, rebuild, stage, swap.
 *
 * Only this class decides what the user is told. It never applies anything
 * itself — the swap is handed to `evohime-transaction.exe`, which keeps its own
 * backup/rollback journal and outlives the shell it replaces.
 *
 * A failed update is never fatal: the currently installed build stays in place
 * and the shell launches as usual with the failure reported in the UI.
 */

export const TRANSACTION_EXECUTABLE = 'evohime-transaction.exe'
export const SHELL_EXECUTABLE = 'EvoHime.exe'

export type GateOutcome = 'continue' | 'applying'

export interface UpdateServiceDeps {
  readonly config: UpdateConfig
  readonly emit: (status: UpdateStatus) => void
  readonly log: (level: 'info' | 'warn' | 'error', event: string, fields: Record<string, unknown>) => void
  readonly quit: () => void
  readonly exists?: (path: string) => boolean
  readonly now?: () => number
  /** Injection points; production uses the real modules. */
  readonly detect?: typeof detectToolchain
  readonly ensure?: typeof ensureToolchain
  readonly remoteHead?: typeof readRemoteHead
  readonly sync?: typeof syncCheckout
  readonly build?: typeof buildStagedPackage
  readonly reset?: typeof clearDerivedState
  readonly buildLog?: BuildLogWriter
  readonly spawnWorker?: (file: string, args: readonly string[]) => void
  readonly setTimer?: (callback: () => void, ms: number) => { readonly cancel: () => void }
}

export class UpdateService {
  private current: UpdateStatus
  private running = false
  private aborter: AbortController | null = null
  private timer: { readonly cancel: () => void } | null = null
  private skipped = false

  constructor(private readonly deps: UpdateServiceDeps) {
    this.current = deps.config.enabled
      ? {
          ...disabledUpdateStatus(deps.config.branch),
          phase: 'idle',
          message: 'Обновления не проверялись.',
          installedCommit: this.installedCommit()
        }
      : disabledUpdateStatus(deps.config.branch)
  }

  get status(): UpdateStatus {
    return this.current
  }

  /**
   * Runs before Core is started: the staged files replace binaries the
   * supervisor would otherwise hold open.
   */
  async runLaunchGate(): Promise<GateOutcome> {
    const { config } = this.deps
    if (!config.enabled || config.launchPolicy === 'off') {
      this.schedulePeriodicCheck()
      return 'continue'
    }

    this.patch({ blocking: true })
    const staged = this.stagedMarker()
    const checked = await this.check()

    if (this.skipped) return this.releaseGate()

    if (checked.phase === 'failed') {
      return this.releaseGate()
    }

    // A background run of the previous session may already have staged exactly
    // the commit we want; then the launch is only a file swap.
    if (staged && staged.commit === checked.remoteCommit && staged.commit !== checked.installedCommit) {
      return this.apply() ? 'applying' : this.releaseGate()
    }

    if (checked.phase === 'up-to-date' || config.launchPolicy === 'apply-ready') {
      return this.releaseGate()
    }

    const prepared = await this.prepare()
    if (this.skipped || prepared.phase !== 'ready') {
      return this.releaseGate()
    }
    return this.apply() ? 'applying' : this.releaseGate()
  }

  /** Compares the installed commit with the tip of the tracked branch. */
  async check(): Promise<UpdateStatus> {
    const { config } = this.deps
    if (!config.enabled) return this.current
    const installedCommit = this.installedCommit()
    this.patch({
      phase: 'checking',
      message: 'Проверяю обновления…',
      error: null,
      installedCommit
    })

    try {
      const toolchain = await (this.deps.detect ?? detectToolchain)({})
      const git = toolPath(toolchain, 'git')
      if (!git) {
        return this.patch({
          phase: 'available',
          message: 'Git не установлен — обновление требует установки инструментов сборки.',
          remoteCommit: null,
          checkedAtMs: this.time()
        })
      }
      const remoteCommit = await (this.deps.remoteHead ?? readRemoteHead)(
        { directory: config.sourceDirectory, repositoryUrl: config.repositoryUrl, branch: config.branch },
        { git }
      )
      const upToDate = installedCommit !== null && installedCommit === remoteCommit
      this.deps.log('info', 'update.checked', { upToDate })
      return this.patch({
        phase: upToDate ? 'up-to-date' : 'available',
        message: upToDate ? 'Установлена последняя версия.' : 'Доступно обновление.',
        remoteCommit,
        checkedAtMs: this.time()
      })
    } catch (error) {
      return this.fail('Не удалось проверить обновления', error)
    }
  }

  /** Installs missing build tools, updates the checkout and rebuilds it. */
  async prepare(): Promise<UpdateStatus> {
    const { config } = this.deps
    if (!config.enabled) return this.current
    if (this.running) return this.current
    this.running = true
    const aborter = new AbortController()
    this.aborter = aborter

    try {
      const target = this.current.remoteCommit ?? (await this.check()).remoteCommit
      if (this.current.phase === 'failed' || !target) return this.current
      if (target === this.current.installedCommit) {
        return this.patch({ phase: 'up-to-date', message: 'Установлена последняя версия.' })
      }
      if (this.stagedMarker()?.commit === target) {
        // A previous run already built this commit; rebuilding it would only
        // burn minutes of the user's machine.
        return this.patch({
          phase: 'ready',
          message: 'Обновление собрано — нужен перезапуск.',
          restartRequired: true
        })
      }

      this.patch({
        phase: 'preparing',
        message: 'Готовлю инструменты сборки…',
        steps: initialUpdateSteps(),
        error: null
      })
      this.deps.buildLog?.start(`EvoHime update ${this.current.installedCommit ?? '—'} → ${target}`)

      const toolchain = await this.prepareToolchain(aborter)
      if (!toolchain) return this.current

      this.step('source', 'active')
      this.patch({ message: 'Получаю исходники…' })
      const git = toolPath(toolchain, 'git')
      if (!git) return this.fail('Git недоступен', new Error('git missing'))
      await (this.deps.sync ?? syncCheckout)(
        { directory: config.sourceDirectory, repositoryUrl: config.repositoryUrl, branch: config.branch },
        target,
        { git, onLine: (line) => this.detail(line) }
      )
      this.step('source', 'done')

      this.patch({ message: 'Пересобираю Еву…' })
      await this.buildOnceOrRetry(target, toolchain, aborter)
      this.step('package', 'done')
      this.deps.log('info', 'update.staged', {})
      return this.patch({
        phase: 'ready',
        message: 'Обновление собрано — нужен перезапуск.',
        restartRequired: true,
        detail: ''
      })
    } catch (error) {
      return this.fail('Сборка обновления не удалась', error)
    } finally {
      this.running = false
      this.aborter = null
    }
  }

  /** Hands the staged package to the transaction worker and quits. */
  restart(): boolean {
    return this.apply()
  }

  /** Abandons a blocking launch rebuild so the installed build can start. */
  skip(): UpdateStatus {
    this.skipped = true
    this.aborter?.abort()
    this.deps.log('info', 'update.skipped', {})
    return this.patch({
      blocking: false,
      message:
        this.current.phase === 'ready'
          ? 'Обновление собрано — применится при следующем перезапуске.'
          : 'Обновление отложено.'
    })
  }

  stop(): void {
    this.timer?.cancel()
    this.timer = null
    this.aborter?.abort()
  }

  /** Background pass for an already running shell: check, then rebuild quietly. */
  private schedulePeriodicCheck(): void {
    const { config } = this.deps
    if (!config.enabled) return
    this.timer?.cancel()
    const schedule = this.deps.setTimer ?? defaultTimer
    this.timer = schedule(() => {
      void this.backgroundPass()
    }, config.checkIntervalMs)
  }

  private async backgroundPass(): Promise<void> {
    if (!this.running && !this.current.restartRequired) {
      const checked = await this.check()
      if (checked.phase === 'available') {
        await this.prepare()
      }
    }
    this.schedulePeriodicCheck()
  }

  /**
   * Builds, and on failure clears the derived state and builds once more.
   *
   * Local builds fail transiently — an interrupted Electron download unpacks
   * into a broken package, and a half-written output directory poisons the next
   * run. Both survive a plain retry, so the second attempt starts from sources
   * and caches only. A second failure is reported as-is.
   */
  private async buildOnceOrRetry(
    commit: string,
    toolchain: ToolchainReport,
    aborter: AbortController
  ): Promise<void> {
    const { config } = this.deps
    const run = (): Promise<unknown> =>
      (this.deps.build ?? buildStagedPackage)(
        {
          sourceDirectory: config.sourceDirectory,
          stagingDirectory: config.stagingDirectory,
          commit,
          branch: config.branch,
          toolchain
        },
        {
          onLine: (line) => this.detail(line),
          onStep: (step) => this.advance(step),
          signal: aborter.signal
        }
      )

    try {
      await run()
    } catch (error) {
      if (aborter.signal.aborted) throw error
      this.deps.log('warn', 'update.build_retry', { reason: redactError(error) })
      this.deps.buildLog?.append(`Сборка не удалась, повторяю с чистым кэшем: ${redactError(error)}`)
      this.patch({ message: 'Сборка не удалась — повторяю с чистым кэшем…', steps: initialUpdateSteps() })
      this.step('toolchain', 'done')
      this.step('source', 'done')
      await (this.deps.reset ?? clearDerivedState)(config.sourceDirectory)
      await run()
    }
  }

  private async prepareToolchain(aborter: AbortController): Promise<ToolchainReport | null> {
    this.step('toolchain', 'active')
    const { report, error } = await (this.deps.ensure ?? ensureToolchain)({
      onLine: (line) => this.detail(line)
    })
    if (aborter.signal.aborted) return null
    if (error) {
      this.step('toolchain', 'failed')
      this.fail('Инструменты сборки не готовы', new Error(error))
      return null
    }
    this.step('toolchain', 'done')
    return report
  }

  private apply(): boolean {
    const { config } = this.deps
    const exists = this.deps.exists ?? existsSync
    const marker = this.stagedMarker()
    const worker = join(config.installDirectory, TRANSACTION_EXECUTABLE)
    if (!marker || !exists(worker)) {
      this.deps.log('warn', 'update.apply_unavailable', { staged: marker !== null })
      return false
    }

    this.patch({
      phase: 'applying',
      message: 'Применяю обновление и перезапускаю…',
      restartRequired: false
    })
    this.step('apply', 'active')
    const args = [
      '--apply-staging',
      '--staging',
      config.stagingDirectory,
      '--install-dir',
      config.installDirectory,
      '--state-dir',
      config.stateDirectory,
      '--wait-pid',
      String(process.pid),
      '--relaunch',
      join(config.installDirectory, SHELL_EXECUTABLE)
    ]
    try {
      ;(this.deps.spawnWorker ?? defaultSpawnWorker)(worker, args)
    } catch (error) {
      // Failing to start the worker must not close the client: the installed
      // build is untouched and still the one that should run.
      this.fail('Не удалось запустить установку обновления', error)
      return false
    }
    this.deps.log('info', 'update.applying', {})
    this.deps.quit()
    return true
  }

  /**
   * Lets the shell start. Every path out of the gate goes through here, so the
   * background pass is always armed afterwards — including after a skip or a
   * failure.
   */
  private releaseGate(): GateOutcome {
    this.patch({ blocking: false })
    this.schedulePeriodicCheck()
    return 'continue'
  }

  private installedCommit(): string | null {
    return readBuildMarker(this.deps.config.installDirectory)?.commit ?? null
  }

  private stagedMarker(): { readonly commit: string } | null {
    const marker = readBuildMarker(this.deps.config.stagingDirectory)
    const exists = this.deps.exists ?? existsSync
    if (!marker || !exists(join(this.deps.config.stagingDirectory, SHELL_EXECUTABLE))) return null
    return marker
  }

  private advance(step: 'core' | 'shell' | 'package'): void {
    if (step === 'shell') this.step('core', 'done')
    if (step === 'package') this.step('shell', 'done')
    this.step(step, 'active')
  }

  private step(id: UpdateStepId, state: UpdateStepState): void {
    const steps: UpdateStep[] = this.current.steps.map((step) =>
      step.id === id ? { ...step, state } : step
    )
    this.patch({ steps })
  }

  private detail(line: string): void {
    this.deps.buildLog?.append(line)
    this.patch({ detail: line })
  }

  private fail(prefix: string, error: unknown): UpdateStatus {
    const message = `${prefix}: ${redactError(error)}`
    this.deps.log('warn', 'update.failed', { reason: message })
    return this.patch({
      phase: 'failed',
      message: prefix,
      error: message,
      steps: this.current.steps.map((step) =>
        step.state === 'active' ? { ...step, state: 'failed' as const } : step
      )
    })
  }

  private patch(changes: Partial<UpdateStatus>): UpdateStatus {
    this.current = { ...this.current, ...changes }
    this.deps.emit(this.current)
    return this.current
  }

  private time(): number {
    return (this.deps.now ?? Date.now)()
  }
}

function defaultTimer(callback: () => void, ms: number): { readonly cancel: () => void } {
  const timer = setTimeout(callback, ms)
  timer.unref?.()
  return { cancel: () => clearTimeout(timer) }
}

/**
 * The worker copies itself out of the installation before touching it, so it
 * must outlive this process: it is detached and its stdio is dropped.
 */
function defaultSpawnWorker(file: string, args: readonly string[]): void {
  const child = spawn(file, [...args], { detached: true, stdio: 'ignore', windowsHide: true })
  // A spawn failure surfaces asynchronously; without a listener it would be an
  // unhandled error event and would take the shell down with it.
  child.once('error', () => {})
  child.unref()
}
