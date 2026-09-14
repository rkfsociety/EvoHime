import { existsSync, readFileSync, writeFileSync, renameSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'

const WINDOW_MS = 10 * 60_000
const MAX_ATTEMPTS = 3
const FILE = 'updater-crash-loop.json'

interface CrashState { readonly firstAtMs: number; readonly attempts: number }

/** Persistent, bounded guard for updater UI/shell restart loops. */
export function recordUpdaterStart(stateDirectory: string, nowMs = Date.now()): { blocked: boolean; attempts: number } {
  mkdirSync(stateDirectory, { recursive: true })
  const path = join(stateDirectory, FILE)
  const previous = readState(path)
  const state = previous && nowMs - previous.firstAtMs < WINDOW_MS
    ? { firstAtMs: previous.firstAtMs, attempts: Math.min(MAX_ATTEMPTS, previous.attempts + 1) }
    : { firstAtMs: nowMs, attempts: 1 }
  writeState(path, state)
  return { blocked: state.attempts >= MAX_ATTEMPTS, attempts: state.attempts }
}

export function clearUpdaterStart(stateDirectory: string): void {
  const path = join(stateDirectory, FILE)
  try { writeFileSync(path, JSON.stringify({ firstAtMs: 0, attempts: 0 }), 'utf8') } catch { /* bounded diagnostic only */ }
}

function readState(path: string): CrashState | null {
  if (!existsSync(path)) return null
  try {
    const value = JSON.parse(readFileSync(path, 'utf8')) as Partial<CrashState>
    if (typeof value.firstAtMs !== 'number' || typeof value.attempts !== 'number' || value.attempts < 0 || value.attempts > MAX_ATTEMPTS) return null
    return { firstAtMs: value.firstAtMs, attempts: value.attempts }
  } catch { return null }
}

function writeState(path: string, state: CrashState): void {
  const temporary = `${path}.tmp`
  try { writeFileSync(temporary, JSON.stringify(state), 'utf8'); renameSync(temporary, path) } catch { /* UI remains bounded if state storage is unavailable */ }
}
