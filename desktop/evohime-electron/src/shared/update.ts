/**
 * Contract of the source-based updater, shared by main, preload and renderer.
 *
 * EvoHime updates from its own git checkout instead of a published release: the
 * shell fetches `origin/<branch>`, rebuilds the product locally and swaps the
 * installed files through `evohime-transaction.exe`. This file must stay free of
 * Electron and Node imports — it is compiled into the sandboxed renderer.
 */

/** Ordered stages of one update run; the UI renders them as a checklist. */
export const UPDATE_STEPS = [
  'toolchain',
  'source',
  'core',
  'shell',
  'package',
  'apply'
] as const

export type UpdateStepId = (typeof UPDATE_STEPS)[number]

export type UpdateStepState = 'pending' | 'active' | 'done' | 'failed' | 'skipped'

export interface UpdateStep {
  readonly id: UpdateStepId
  readonly label: string
  readonly state: UpdateStepState
}

/**
 * Where the update run currently is.
 *
 * `ready` means a rebuilt package is staged and only the restart is missing;
 * `applying` means the transaction worker was handed the staged package and the
 * shell is about to exit.
 */
export type UpdatePhase =
  | 'disabled'
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'preparing'
  | 'ready'
  | 'applying'
  | 'failed'

export interface UpdateStatus {
  readonly phase: UpdatePhase
  /**
   * True while the shell holds the UI back until the run finishes — the launch
   * rebuild. A background rebuild of a running shell is never blocking.
   */
  readonly blocking: boolean
  /** Short Russian sentence for the status bar. */
  readonly message: string
  /** Last redacted build output line, or an empty string. */
  readonly detail: string
  readonly steps: readonly UpdateStep[]
  /** Commit currently installed, as recorded by the last successful build. */
  readonly installedCommit: string | null
  /** Newest commit on the tracked branch, once a check succeeded. */
  readonly remoteCommit: string | null
  readonly branch: string
  /** Redacted failure reason, present only in the `failed` phase. */
  readonly error: string | null
  readonly checkedAtMs: number | null
  /** True once a staged package is waiting for the restart. */
  readonly restartRequired: boolean
}

export const UPDATE_STEP_LABELS: Record<UpdateStepId, string> = {
  toolchain: 'Инструменты сборки',
  source: 'Исходники',
  core: 'Сборка Core',
  shell: 'Сборка оболочки',
  package: 'Упаковка',
  apply: 'Применение'
}

export const UPDATE_STEP_DESCRIPTIONS: Record<UpdateStepId, string> = {
  toolchain: 'Проверяю и подготавливаю Git, Rust и Node.js.',
  source: 'Синхронизирую локальную копию с выбранным commit.',
  core: 'Компилирую Rust Core и supervisor.',
  shell: 'Собираю Electron-оболочку приложения.',
  package: 'Формирую переносимый Windows-пакет.',
  apply: 'Передаю пакет установщику и перезапускаю приложение.'
}

export function initialUpdateSteps(): readonly UpdateStep[] {
  return UPDATE_STEPS.map((id) => ({ id, label: UPDATE_STEP_LABELS[id], state: 'pending' as const }))
}

export function disabledUpdateStatus(branch = 'main'): UpdateStatus {
  return {
    phase: 'disabled',
    blocking: false,
    message: 'Автообновление выключено.',
    detail: '',
    steps: initialUpdateSteps(),
    installedCommit: null,
    remoteCommit: null,
    branch,
    error: null,
    checkedAtMs: null,
    restartRequired: false
  }
}

/**
 * Short form of a commit for the UI. Versions are only a label for the
 * installer — what an installation actually is, is the commit it was built from.
 */
export function shortCommit(commit: string | null): string {
  return commit ? commit.slice(0, 7) : '—'
}

/** Fraction of the run that is done, or `null` while nothing started. */
export function updateProgress(status: UpdateStatus): number | null {
  const done = status.steps.filter((step) => step.state === 'done' || step.state === 'skipped').length
  const active = status.steps.some((step) => step.state === 'active')
  if (done === 0 && !active) return null
  return Math.min(1, (done + (active ? 0.5 : 0)) / status.steps.length)
}

export function completedUpdateSteps(status: UpdateStatus): number {
  return status.steps.filter((step) => step.state === 'done' || step.state === 'skipped').length
}

export function activeUpdateStep(status: UpdateStatus): UpdateStep | null {
  return status.steps.find((step) => step.state === 'active') ?? null
}
