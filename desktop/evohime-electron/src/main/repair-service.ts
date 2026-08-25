import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { randomUUID } from 'node:crypto'

import type { CoreEvent } from '@shared/api'
import type { RepairEvidenceEntry, RepairPhase, RepairStatus, RepairTestResult } from '@shared/api'

import type { ShellLog } from './diagnostics/logger'
import { githubApiBase, readCommitState } from './update/commit-status'
import { resolveGithubToken } from './update/github-token'
import type { UpdateConfig } from './update/config'
import { readRemoteHead, syncCheckout } from './update/source-checkout'
import { describeFailure, runCommand } from './update/run-command'

const ERROR_THRESHOLD = 3
const MAX_SUMMARY_CHARS = 240

export interface RepairServiceOptions {
  readonly filePath: string
  readonly repairRoot: string
  readonly config: UpdateConfig
  readonly startTask: (taskId: string, workspacePath: string, prompt: string) => boolean
  readonly stopTask: (taskId: string) => boolean
  readonly emit: (status: RepairStatus) => void
  readonly log: ShellLog
  readonly fetch?: typeof globalThis.fetch
}

type RepairOperation = 'diagnose' | 'commit' | 'push'

export class RepairService {
  private current: RepairStatus
  private operation: RepairOperation | null = null
  private readonly errorPatterns = new Map<string, number>()

  constructor(private readonly options: RepairServiceOptions) {
    this.current = readStatus(options.filePath)
    if (['preparing', 'diagnosing', 'committing', 'pushing', 'waiting_ci'].includes(this.current.phase)) {
      this.current = {
        ...this.current,
        phase: 'failed',
        taskId: null,
        error: 'Предыдущий repair-run прерван закрытием Евы; запусти его снова кнопкой Починить.',
        summary: 'Предыдущий repair-run был прерван.',
        updatedAtMs: Date.now()
      }
      writeStatus(options.filePath, this.current)
    }
  }

  get status(): RepairStatus {
    return this.current
  }

  observe(event: CoreEvent): void {
    if (event.eventType === 'task.failed') {
      const summary = extractError(event.payload)
      const pattern = (summary.split(':').at(0) ?? '').trim().slice(0, 80) || 'unknown'
      this.errorPatterns.set(pattern, (this.errorPatterns.get(pattern) ?? 0) + 1)
      const nextCount = this.current.errorCount + 1
      this.set({
        errorCount: nextCount,
        repeatedPatterns: [...this.errorPatterns.values()].filter((count) => count > 1).length,
        summary,
        phase: this.isActive() ? this.current.phase : nextCount >= ERROR_THRESHOLD ? 'available' : 'idle'
      })
    }

    if (!this.current.taskId || event.taskId !== this.current.taskId) return
    if (event.eventType === 'task.completed') {
      if (this.operation === 'diagnose') {
        void this.inspectEvidence(extractTests(event.payload))
      }
      if (this.operation === 'commit') this.set({ phase: 'ready_to_push', summary: 'Commit создан. Push требует отдельного подтверждения.' })
      if (this.operation === 'push') this.set({ phase: 'waiting_ci', summary: 'Push завершён. Ожидаю GitHub Actions.', ciState: 'pending' })
      this.operation = null
    } else if (event.eventType === 'task.failed') {
      this.set({ phase: 'failed', error: extractError(event.payload) })
      this.operation = null
    }
  }

  async start(workspacePath: string): Promise<RepairStatus> {
    if (this.isActive()) return this.current
    if (this.current.errorCount < ERROR_THRESHOLD) {
      return this.fail('Пока недостаточно повторяющихся ошибок для repair-run.')
    }
    if (workspacePath.trim().length === 0) {
      return this.fail('Сначала выбери workspace проекта.')
    }
    const repairId = randomUUID()
    // The checkout is isolated, so the only branch that can be published by a
    // separately approved push is the configured product branch.
    const branch = this.options.config.branch
    const directory = join(this.options.repairRoot, repairId)
    const taskId = randomUUID()
    this.set({
      phase: 'preparing', repairId, workspacePath: directory, baseCommit: null, branch,
      taskId, commit: null, ciState: 'unknown', diffStat: '', tests: [], error: null,
      summary: 'Готовлю изолированную копию репозитория…'
    })

    try {
      const remote = githubApiBase(this.options.config.repositoryUrl)
      if (!remote || this.options.config.repositoryUrl !== 'https://github.com/rkfsociety/EvoHime.git') {
        throw new Error('Repair разрешён только для канонического репозитория EvoHime.')
      }
      const selectedRemote = await runCommand({
        file: 'git', args: ['-C', workspacePath, 'remote', 'get-url', 'origin'],
        cwd: workspacePath, capture: true, timeoutMs: 20_000
      })
      const selectedUrl = selectedRemote.code === 0 ? selectedRemote.raw.at(-1)?.trim() ?? '' : ''
      if (normalizeRemote(selectedUrl) !== normalizeRemote(this.options.config.repositoryUrl)) {
        throw new Error('Выбранный workspace не является исходным репозиторием EvoHime.')
      }
      const baseCommit = await readRemoteHead(
        { directory, repositoryUrl: this.options.config.repositoryUrl, branch: this.options.config.branch },
        { git: 'git' }
      )
      await syncCheckout(
        { directory, repositoryUrl: this.options.config.repositoryUrl, branch: this.options.config.branch },
        baseCommit,
        { git: 'git' }
      )
      const branchResult = await runCommand({
        file: 'git', args: ['checkout', '-B', branch, baseCommit], cwd: directory,
        timeoutMs: 60_000, capture: true
      })
      if (branchResult.code !== 0) throw new Error(describeFailure('Создание repair-ветки', branchResult))
      this.set({ phase: 'diagnosing', baseCommit, summary: 'Ева анализирует ошибки и готовит исправление.' })
      this.operation = 'diagnose'
      const queued = this.options.startTask(taskId, directory, repairPrompt(this.current.summary, baseCommit))
      if (!queued) throw new Error('Core не принял repair-задачу.')
    } catch (error) {
      this.operation = null
      this.set({ phase: 'failed', error: safeError(error), summary: 'Repair-run не запустился.' })
    }
    return this.current
  }

  cancel(): RepairStatus {
    if (this.current.taskId) this.options.stopTask(this.current.taskId)
    this.operation = null
    this.set({ phase: 'cancelled', summary: 'Repair-run отменён пользователем.' })
    return this.current
  }

  commit(): RepairStatus {
    if (this.current.phase !== 'ready_to_commit' || !this.current.workspacePath || !this.current.taskId) return this.current
    this.operation = 'commit'
    const taskId = randomUUID()
    this.set({ phase: 'committing', taskId, summary: 'Ева проверяет diff и готовит commit.' })
    const queued = this.options.startTask(taskId, this.current.workspacePath, commitPrompt(this.current.branch ?? ''))
    if (!queued) {
      this.operation = null
      return this.fail('Core не принял commit-задачу.')
    }
    return this.current
  }

  push(): RepairStatus {
    if (this.current.phase !== 'ready_to_push' || !this.current.workspacePath || !this.current.taskId) return this.current
    this.operation = 'push'
    const taskId = randomUUID()
    this.set({ phase: 'pushing', taskId, summary: 'Ожидаю approval и отправляю repair commit в GitHub.' })
    const queued = this.options.startTask(taskId, this.current.workspacePath, pushPrompt())
    if (!queued) {
      this.operation = null
      return this.fail('Core не принял push-задачу.')
    }
    return this.current
  }

  async refreshCi(): Promise<RepairStatus> {
    if (!this.current.workspacePath || this.current.phase !== 'waiting_ci') return this.current
    try {
      const result = await runCommand({ file: 'git', args: ['rev-parse', 'HEAD'], cwd: this.current.workspacePath, capture: true, timeoutMs: 20_000 })
      const commit = result.code === 0 ? result.raw.at(-1)?.trim() ?? null : null
      const apiBase = githubApiBase(this.options.config.repositoryUrl)
      if (!commit || !apiBase) return this.set({ ciState: 'unknown', error: 'GitHub Actions пока недоступны.' })
      const token = await resolveGithubToken({ configured: this.options.config.githubToken })
      const ciDeps = token?.token ? { token: token.token } : {}
      const deps = this.options.fetch ? { ...ciDeps, fetch: this.options.fetch } : ciDeps
      const ciState = await readCommitState(apiBase, commit, deps)
      const phase: RepairPhase = ciState === 'success' ? 'ready_to_update' : 'waiting_ci'
      return this.set({ commit, ciState, phase, error: ciState === 'failure' ? 'GitHub Actions завершились с ошибкой.' : null, summary: ciState === 'success' ? 'Commit зелёный. Можно проверять и применять обновление.' : 'Ожидаю завершения GitHub Actions.' })
    } catch (error) {
      return this.set({ ciState: 'unknown', error: `Проверка CI отложена: ${safeError(error)}` })
    }
  }

  private async inspectEvidence(tests: RepairTestResult[]): Promise<void> {
    if (!this.current.workspacePath) return
    try {
      const [statResult, filesResult] = await Promise.all([
        runCommand({
        file: 'git', args: ['diff', '--stat'], cwd: this.current.workspacePath,
        capture: true, timeoutMs: 20_000
        }),
        runCommand({
          file: 'git', args: ['diff', '--name-only'], cwd: this.current.workspacePath,
          capture: true, timeoutMs: 20_000
        })
      ])
      const diffStat = statResult.code === 0 ? statResult.tail.join(' ').slice(0, 16_384) : 'Не удалось прочитать diff.'
      const changedFiles = filesResult.code === 0 ? filesResult.raw.map((file) => file.trim()).filter(Boolean) : []
      const protectedFile = changedFiles.find(isProtectedRepairPath)
      if (protectedFile) {
        this.set({ phase: 'failed', tests, diffStat, error: `Изменён защищённый файл: ${protectedFile}. Нужен ручной review.` , summary: 'Repair-run остановлен на защищённом контуре.' })
        this.operation = null
        return
      }
      this.set({ phase: 'ready_to_commit', tests, diffStat, summary: 'Диагностика и исправление завершены. Проверь diff и тесты.' })
    } catch {
      this.set({ phase: 'failed', tests, diffStat: 'Не удалось прочитать diff.', error: 'Не удалось получить bounded diff для review.' })
      this.operation = null
    }
  }

  private isActive(): boolean {
    return ['preparing', 'diagnosing', 'committing', 'pushing', 'waiting_ci'].includes(this.current.phase)
  }

  private set(patch: Partial<RepairStatus>): RepairStatus {
    const updatedAtMs = Date.now()
    const next = { ...this.current, ...patch, updatedAtMs }
    const phaseChanged = patch.phase !== undefined && patch.phase !== this.current.phase
    const outcomeChanged = patch.error !== undefined || patch.ciState !== undefined
    if (phaseChanged || outcomeChanged) {
      const result: RepairEvidenceEntry['result'] = next.error
        ? 'failed'
        : next.phase === 'cancelled'
          ? 'cancelled'
          : ['ready_to_commit', 'ready_to_push', 'ready_to_update'].includes(next.phase)
            ? 'passed'
            : 'pending'
      const entry: RepairEvidenceEntry = {
        phase: next.phase,
        atMs: updatedAtMs,
        result,
        commit: next.commit,
        ciState: next.ciState,
        detail: next.summary.replace(/[\r\n]/g, ' ').slice(0, MAX_SUMMARY_CHARS)
      }
      next.evidence = [...(this.current.evidence ?? []), entry].slice(-64)
    }
    this.current = next
    writeStatus(this.options.filePath, this.current)
    this.options.emit(this.current)
    return this.current
  }

  private fail(error: string): RepairStatus {
    this.options.log('warn', 'shell.repair_failed', { reason: error.slice(0, 160) })
    return this.set({ phase: 'failed', error: error.slice(0, 240), summary: 'Repair-run остановлен.' })
  }
}

function emptyStatus(): RepairStatus {
  return { phase: 'idle', repairId: null, workspacePath: null, baseCommit: null, branch: null, taskId: null, errorCount: 0, repeatedPatterns: 0, summary: 'Ошибок для repair-run пока нет.', diffStat: '', tests: [], commit: null, ciState: 'unknown', error: null, updatedAtMs: 0, evidence: [] }
}

function readStatus(filePath: string): RepairStatus {
  try {
    const value = JSON.parse(readFileSync(filePath, 'utf8')) as Partial<RepairStatus>
    return { ...emptyStatus(), ...value }
  } catch {
    return emptyStatus()
  }
}

function writeStatus(filePath: string, value: RepairStatus): void {
  mkdirSync(dirname(filePath), { recursive: true })
  const temporary = `${filePath}.tmp`
  writeFileSync(temporary, JSON.stringify(value), { encoding: 'utf8', mode: 0o600 })
  renameSync(temporary, filePath)
}

function extractError(payload: string): string {
  try {
    const value = JSON.parse(payload) as Record<string, unknown>
    if (typeof value.error === 'string') return value.error.replace(/[\r\n]/g, ' ').slice(0, MAX_SUMMARY_CHARS)
  } catch { /* redacted event may be plain text */ }
  return payload.replace(/[\r\n]/g, ' ').slice(0, MAX_SUMMARY_CHARS) || 'Неизвестная ошибка задачи.'
}

function safeError(error: unknown): string {
  return (error instanceof Error ? error.message : String(error)).replace(/[\r\n]/g, ' ').slice(0, MAX_SUMMARY_CHARS)
}

function normalizeRemote(value: string): string {
  return value.trim().replace(/\.git$/i, '').replace(/^git@github\.com:/i, 'https://github.com/').replace(/\/$/, '').toLowerCase()
}

function isProtectedRepairPath(path: string): boolean {
  const normalized = path.replaceAll('\\', '/').replace(/^\.\//, '').toLowerCase()
  return normalized === 'agents.md' || normalized.startsWith('.codex/') ||
    normalized.startsWith('.github/workflows/') || normalized.startsWith('installer/') ||
    normalized.startsWith('crates/evohime-updater/') || normalized.includes('supervisor') ||
    normalized.includes('receipt') || normalized.includes('security') || normalized.startsWith('.env')
}

function extractTests(payload: string): RepairTestResult[] {
  try {
    const value = JSON.parse(payload) as Record<string, unknown>
    const message = typeof value.final_message === 'string' ? value.final_message : ''
    const lines = message.split(/\r?\n/).map((line) => line.trim()).filter((line) => /\b(test|cargo|npm|vitest|diff --check)\b/i.test(line))
    return lines.slice(0, 12).map((line) => ({
      name: 'Core validation',
      state: /fail|error|panic/i.test(line) ? 'failed' : 'passed',
      detail: line.slice(0, MAX_SUMMARY_CHARS)
    }))
  } catch {
    return []
  }
}

function repairPrompt(summary: string, baseCommit: string): string {
  return `Пользователь запустил repair-run EvoHime для собственного репозитория. Базовый commit: ${baseCommit}. Найди и исправь причину накопившихся ошибок: ${summary}. Работай только в текущем workspace. Сначала изучи фактические файлы и git status/diff, затем внеси минимальный patch через filesystem.patch или filesystem.write, запусти подходящие тесты и git diff --check. Не выполняй git.commit, git.push, удаление веток, изменение GitHub Actions, policy, updater, supervisor, receipt-кода или секретных файлов: commit и push будут отдельными действиями пользователя. Не объявляй успех без фактического результата тестов.`
}

function commitPrompt(branch: string): string {
  return `Пользователь подтвердил commit repair-run в ветке ${branch}. Проверь git.status и git.diff, убедись, что изменены только относящиеся к ошибке файлы, запусти обязательные targeted checks и git diff --check. Затем выполни git.commit с task-only сообщением. Не выполняй git.push и не меняй release/security policy.`
}

function pushPrompt(): string {
  return 'Пользователь отдельно подтвердил публикацию repair commit в origin/main. Проверь текущий commit и remote, затем выполни только обычный git.push с remote=origin и branch=main. Force push, no-verify, удаление веток и изменение содержимого запрещены. После push сообщи точный commit SHA и не считай CI успешным без проверки GitHub Actions.'
}
