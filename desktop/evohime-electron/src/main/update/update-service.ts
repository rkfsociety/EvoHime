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
import { runBuildWorker } from './build-worker'
import {
  githubApiBase,
  selectGreenCommit,
  touchesProductCode,
  type CommitCheckState
} from './commit-status'
import { readBuildMarker, type UpdateConfig } from './config'
import { resolveGithubToken } from './github-token'
import { downloadReleaseInstaller } from './release-installer'
import { readRemoteHead, syncCheckout } from './source-checkout'
import { detectToolchain, ensureToolchain, toolPath, type ToolchainReport } from './toolchain'

/**
 * Orchestrates the update: check, download/build, stage, swap.
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
  readonly selectGreen?: typeof selectGreenCommit
  readonly productChanges?: typeof touchesProductCode
  readonly resolveToken?: typeof resolveGithubToken
  readonly sync?: typeof syncCheckout
  readonly build?: typeof buildStagedPackage
  readonly downloadInstaller?: typeof downloadReleaseInstaller
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
  private token: Promise<string | null> | null = null

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

    // Installer updates are intentionally background-only: starting the shell
    // must never wait for GitHub or for a multi-hundred-megabyte download.
    // The periodic pass downloads the verified installer and then notifies the
    // user that a restart is ready.
    if (config.launchPolicy === 'installer') {
      const stagedInstaller = this.stagedInstallerMarker()
      if (stagedInstaller && stagedInstaller.commit === checked.remoteCommit) {
        this.patch({
          phase: 'ready',
          message: 'Проверенный установщик уже скачан — можно перезапустить Еву.',
          restartRequired: true
        })
      }
      return this.releaseGate()
    }

    // A background run of the previous session may already have staged exactly
    // the commit we want; then the launch is only a file swap.
    const stagedInstaller = this.stagedInstallerMarker()
    if (stagedInstaller && stagedInstaller.commit === checked.remoteCommit && stagedInstaller.commit !== checked.installedCommit) {
      return this.apply() ? 'applying' : this.releaseGate()
    }
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
      const tip = await (this.deps.remoteHead ?? readRemoteHead)(
        { directory: config.sourceDirectory, repositoryUrl: config.repositoryUrl, branch: config.branch },
        { git }
      )
      const candidate = await this.greenCandidate(tip)
      if (!candidate.commit) {
        this.deps.log('info', 'update.checked', { green: false })
        return this.patch({
          phase: 'up-to-date',
          message: candidate.message,
          remoteCommit: null,
          checkedAtMs: this.time()
        })
      }

      if (installedCommit !== null && installedCommit === candidate.commit) {
        this.deps.log('info', 'update.checked', { upToDate: true, green: true })
        return this.patch({
          phase: 'up-to-date',
          message: 'Установлена последняя версия.',
          remoteCommit: candidate.commit,
          checkedAtMs: this.time()
        })
      }

      if (installedCommit !== null && !(await this.changesProduct(installedCommit, candidate.commit))) {
        // Документация и планы не меняют собранный клиент: тратить на них
        // минуты пересборки незачем.
        this.deps.log('info', 'update.checked', { upToDate: true, documentationOnly: true })
        return this.patch({
          phase: 'up-to-date',
          message: 'Новые коммиты не трогают код клиента — пересборка не нужна.',
          remoteCommit: candidate.commit,
          checkedAtMs: this.time()
        })
      }

      this.deps.log('info', 'update.checked', { upToDate: false, green: true })
      return this.patch({
        phase: 'available',
        message: candidate.message,
        remoteCommit: candidate.commit,
        checkedAtMs: this.time()
      })
    } catch (error) {
      return this.fail('Не удалось проверить обновления', error)
    }
  }

  /**
   * Credential for the GitHub API, looked up once per process.
   *
   * Anonymous checks share a 60-per-hour budget with everything else behind the
   * same address, and a check that ran out of budget looks to the user like a
   * broken updater. The lookup can spawn `gh`, so its result is cached, and a
   * failed lookup is not an error: the check simply runs anonymously.
   */
  private githubToken(): Promise<string | null> {
    this.token ??= (this.deps.resolveToken ?? resolveGithubToken)({
      configured: this.deps.config.githubToken
    })
      .then((found) => {
        this.deps.log('info', 'update.github_token', { source: found?.source ?? 'none' })
        return found?.token ?? null
      })
      .catch((error) => {
        this.deps.log('warn', 'update.github_token_failed', { reason: redactError(error) })
        return null
      })
    return this.token
  }

  /**
   * Commit the client is allowed to move to.
   *
   * With `requireGreenCommit` the branch tip is not enough: the rebuild happens
   * on the user's machine, so a commit that failed CI would be compiled and
   * installed locally. While checks run on the tip, the newest already-green
   * commit is taken instead, so a push does not stall the client for the length
   * of a CI run. A repository whose checks cannot be read is never assumed
   * green — the client simply stays where it is.
   */
  private async greenCandidate(
    tip: string
  ): Promise<{ readonly commit: string | null; readonly message: string }> {
    const { config } = this.deps
    if (!config.requireGreenCommit) {
      return { commit: tip, message: 'Доступно обновление.' }
    }

    const apiBase = githubApiBase(config.repositoryUrl)
    if (!apiBase) {
      this.deps.log('warn', 'update.checks_unavailable', {})
      return {
        commit: null,
        message: 'Проверки коммитов недоступны для этого репозитория — обновление пропущено.'
      }
    }

    const selected = await (this.deps.selectGreen ?? selectGreenCommit)(
      apiBase,
      config.branch,
      config.greenCommitDepth,
      { token: await this.githubToken() }
    )
    if (!selected.commit) {
      return { commit: null, message: waitingMessage(selected.tipState) }
    }
    return {
      commit: selected.commit,
      message:
        selected.commit === tip
          ? 'Доступно обновление.'
          : 'Доступно обновление до последнего зелёного коммита.'
    }
  }

  /**
   * Whether the range between two commits can change the built client.
   *
   * Any doubt — an unreadable range, an unfamiliar host — answers "yes": an
   * extra rebuild costs minutes, a skipped one leaves the user on stale code.
   */
  private async changesProduct(installed: string, target: string): Promise<boolean> {
    const apiBase = githubApiBase(this.deps.config.repositoryUrl)
    if (!apiBase) return true
    try {
      return await (this.deps.productChanges ?? touchesProductCode)(apiBase, installed, target, {
        token: await this.githubToken()
      })
    } catch (error) {
      this.deps.log('warn', 'update.compare_failed', { reason: redactError(error) })
      return true
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
      // Прошлая неудача не должна блокировать повтор: «failed» из предыдущей
      // попытки уже показан в UI, и кнопка «Повторить» ведёт сюда же. Отменяет
      // сборку только та проверка, которую мы запустили прямо сейчас.
      let target = this.current.remoteCommit
      if (!target) {
        const checked = await this.check()
        if (checked.phase === 'failed') return this.current
        target = checked.remoteCommit
      }
      if (!target) return this.current
      if (target === this.current.installedCommit) {
        return this.patch({ phase: 'up-to-date', message: 'Установлена последняя версия.' })
      }
      if (config.launchPolicy === 'installer') {
        return this.prepareInstaller(target)
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

  /** Downloads the installer published by CI after its checks passed. */
  private async prepareInstaller(target: string): Promise<UpdateStatus> {
    const { config } = this.deps
    this.patch({
      phase: 'preparing',
      message: 'Скачиваю проверенный установщик…',
      steps: initialUpdateSteps(),
      error: null
    })
    this.step('package', 'active')
    try {
      const downloaded = await (this.deps.downloadInstaller ?? downloadReleaseInstaller)(
        config.repositoryUrl,
        config.branch,
        target,
        config.stagingDirectory,
        await this.githubToken()
      )
      this.step('package', 'done')
      this.deps.log('info', 'update.installer_downloaded', { commit: downloaded.marker.commit })
      return this.patch({
        phase: 'ready',
        message: 'Установщик скачан — нужен перезапуск.',
        restartRequired: true,
        detail: ''
      })
    } catch (error) {
      this.step('package', 'failed')
      return this.fail('Скачивание установщика не удалось', error)
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
      (this.deps.build ?? runBuildWorker)(
        {
          sourceDirectory: config.sourceDirectory,
          stagingDirectory: config.stagingDirectory,
          installDirectory: config.installDirectory,
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
    const installerMarker = this.stagedInstallerMarker()
    const worker = join(config.installDirectory, TRANSACTION_EXECUTABLE)
    if ((!marker && !installerMarker) || !exists(worker)) {
      this.deps.log('warn', 'update.apply_unavailable', { staged: marker !== null, installer: installerMarker !== null })
      return false
    }

    this.patch({
      phase: 'applying',
      message: 'Применяю обновление и перезапускаю…',
      restartRequired: false
    })
    this.step('apply', 'active')
    const args = installerMarker
      ? [
          '--installer', join(config.stagingDirectory, 'EvoHime-Setup.exe'),
          '--install-dir', config.installDirectory,
          '--state-dir', config.stateDirectory,
          '--relaunch', join(config.installDirectory, SHELL_EXECUTABLE)
        ]
      : [
          '--apply-staging', '--staging', config.stagingDirectory,
          '--install-dir', config.installDirectory, '--state-dir', config.stateDirectory,
          '--wait-pid', String(process.pid), '--relaunch', join(config.installDirectory, SHELL_EXECUTABLE)
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

  private stagedInstallerMarker(): { readonly commit: string } | null {
    const marker = readBuildMarker(this.deps.config.stagingDirectory)
    const exists = this.deps.exists ?? existsSync
    if (!marker || !exists(join(this.deps.config.stagingDirectory, 'EvoHime-Setup.exe'))) return null
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

/** Says why nothing is being installed, without claiming more than is known. */
function waitingMessage(tipState: CommitCheckState): string {
  switch (tipState) {
    case 'pending':
      return 'Свежий коммит ещё проверяется — обновлюсь, когда сборка станет зелёной.'
    case 'failure':
      return 'Свежие коммиты не прошли проверки — обновление отложено.'
    default:
      // A cancelled or missing run is not a failure: the verdict simply never
      // arrived, and an unverified commit is not installed.
      return 'Проверки свежих коммитов не завершились — обновление отложено.'
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
