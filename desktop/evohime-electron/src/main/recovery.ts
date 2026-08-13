/**
 * Bounded renderer/preload reload policy (plan 0, stage 3).
 *
 * Automatic reloads are allowed at most N times inside a sliding window of T
 * milliseconds. Past the threshold the shell stops reloading and shows a
 * minimal recovery surface instead of looping forever.
 */

export const DEFAULT_MAX_RELOADS = 3
export const DEFAULT_RELOAD_WINDOW_MS = 60_000

export type ReloadDecision = 'reload' | 'recovery-window'

export class ReloadLimiter {
  private readonly failures: number[] = []

  constructor(
    private readonly maxReloads = DEFAULT_MAX_RELOADS,
    private readonly windowMs = DEFAULT_RELOAD_WINDOW_MS,
    private readonly now: () => number = Date.now
  ) {}

  record(): ReloadDecision {
    const current = this.now()
    while (this.failures.length > 0 && current - (this.failures[0] as number) > this.windowMs) {
      this.failures.shift()
    }
    this.failures.push(current)
    return this.failures.length > this.maxReloads ? 'recovery-window' : 'reload'
  }

  reset(): void {
    this.failures.length = 0
  }

  get recentFailures(): number {
    return this.failures.length
  }
}
