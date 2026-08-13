/** Deterministic bounded exponential backoff used by reconnect and retry. */

export interface BackoffOptions {
  readonly baseMs: number
  readonly maxMs: number
  /** Fraction of the delay that may be added as jitter, 0 disables jitter. */
  readonly jitterRatio: number
}

export const DEFAULT_BACKOFF: BackoffOptions = {
  baseMs: 250,
  maxMs: 10_000,
  jitterRatio: 0.2
}

export function backoffDelayMs(
  attempt: number,
  options: BackoffOptions = DEFAULT_BACKOFF,
  random: () => number = Math.random
): number {
  const safeAttempt = Math.max(0, Math.floor(attempt))
  const exponential = options.baseMs * 2 ** Math.min(safeAttempt, 20)
  const capped = Math.min(exponential, options.maxMs)
  const jitter = capped * options.jitterRatio * random()
  return Math.round(Math.min(capped + jitter, options.maxMs))
}
