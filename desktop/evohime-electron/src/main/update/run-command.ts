import { spawn } from 'node:child_process'

import { redactText } from '../diagnostics/redact'

/**
 * Bounded child-process runner used by the updater.
 *
 * Build tools are long-running and chatty, so output is streamed line by line,
 * redacted before it can reach the UI, and only a small tail is kept in memory.
 * Nothing here goes through a shell: every command is an executable plus an
 * argument vector, so a value from the config file cannot become a command.
 */

export interface RunOptions {
  readonly file: string
  readonly args: readonly string[]
  readonly cwd?: string
  readonly env?: NodeJS.ProcessEnv
  readonly timeoutMs?: number
  /** Redacted output lines, in order. */
  readonly onLine?: (line: string) => void
  readonly signal?: AbortSignal
  /**
   * Keeps an unredacted tail in `raw`. Only git plumbing uses it — redaction
   * mangles commit object names — and its value must never reach the renderer.
   */
  readonly capture?: boolean
}

export interface RunResult {
  readonly code: number
  /** Redacted tail of the combined output. */
  readonly tail: readonly string[]
  /** Unredacted tail, populated only when `capture` was requested. */
  readonly raw: readonly string[]
  readonly timedOut: boolean
}

export type CommandRunner = (options: RunOptions) => Promise<RunResult>

export const MAX_TAIL_LINES = 40
export const DEFAULT_TIMEOUT_MS = 60 * 60_000

export const runCommand: CommandRunner = (options) =>
  new Promise<RunResult>((resolve) => {
    const tail: string[] = []
    const raw: string[] = []
    let settled = false
    let timedOut = false

    const child = spawn(options.file, [...options.args], {
      cwd: options.cwd ?? process.cwd(),
      env: options.env ?? process.env,
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe']
    })

    const finish = (code: number): void => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      options.signal?.removeEventListener('abort', onAbort)
      resolve({ code, tail, raw, timedOut })
    }

    const timer = setTimeout(() => {
      timedOut = true
      child.kill()
      finish(-1)
    }, options.timeoutMs ?? DEFAULT_TIMEOUT_MS)
    timer.unref?.()

    const onAbort = (): void => {
      child.kill()
      finish(-1)
    }
    options.signal?.addEventListener('abort', onAbort, { once: true })

    const consume = (chunk: Buffer | string): void => {
      for (const rawLine of String(chunk).split(/\r?\n/)) {
        const line = rawLine.trim()
        if (line.length === 0) continue
        if (options.capture) {
          raw.push(line)
          if (raw.length > MAX_TAIL_LINES) raw.shift()
        }
        const safe = redactText(line)
        tail.push(safe)
        if (tail.length > MAX_TAIL_LINES) tail.shift()
        options.onLine?.(safe)
      }
    }

    child.stdout?.on('data', consume)
    child.stderr?.on('data', consume)
    child.once('error', (error) => {
      consume(`${error}`)
      finish(-1)
    })
    child.once('close', (code) => finish(code ?? -1))
  })

/** Fails with a redacted, bounded message so callers can surface it directly. */
export function describeFailure(label: string, result: RunResult): string {
  if (result.timedOut) return `${label}: превышено время выполнения.`
  const lastLine = result.tail.at(-1) ?? ''
  return lastLine ? `${label}: ${lastLine}` : `${label}: код ${result.code}.`
}
