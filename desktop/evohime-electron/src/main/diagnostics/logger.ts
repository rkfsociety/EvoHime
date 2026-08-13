import { appendFileSync, existsSync, mkdirSync, renameSync, rmSync, statSync } from 'node:fs'
import { join } from 'node:path'

import { redactText, redactValue, type RedactedValue } from './redact'

/**
 * Redacted JSONL diagnostics for the Electron shell.
 *
 * Shell streams stay separate from the Core journal, which remains
 * authoritative for agent events (plan 0, stage 4). Files rotate by size with
 * a bounded number of generations so a crash loop cannot fill the disk.
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error'

/** Signature every component uses to emit a redacted shell diagnostic. */
export type ShellLog = (
  level: LogLevel,
  event: string,
  fields?: Record<string, unknown>
) => void

export type ShellLogStream = 'main' | 'renderer'

export interface JsonlLoggerOptions {
  readonly directory: string
  readonly stream: ShellLogStream
  readonly maxBytes?: number
  readonly maxFiles?: number
  /** Injectable for tests; production passes the real clock. */
  readonly now?: () => Date
}

export const DEFAULT_MAX_LOG_BYTES = 4 * 1024 * 1024
export const DEFAULT_MAX_LOG_FILES = 3

export class JsonlLogger {
  private readonly filePath: string
  private readonly maxBytes: number
  private readonly maxFiles: number
  private readonly now: () => Date
  private disabled = false

  constructor(private readonly options: JsonlLoggerOptions) {
    this.filePath = join(options.directory, `shell-${options.stream}.jsonl`)
    this.maxBytes = options.maxBytes ?? DEFAULT_MAX_LOG_BYTES
    this.maxFiles = options.maxFiles ?? DEFAULT_MAX_LOG_FILES
    this.now = options.now ?? (() => new Date())
  }

  write(level: LogLevel, event: string, fields: Record<string, unknown> = {}): void {
    if (this.disabled) {
      return
    }
    const record: Record<string, RedactedValue> = {
      ts: this.now().toISOString(),
      level,
      stream: this.options.stream,
      event: redactText(event),
      ...(redactValue(fields) as Record<string, RedactedValue>)
    }
    try {
      mkdirSync(this.options.directory, { recursive: true })
      this.rotateIfNeeded()
      appendFileSync(this.filePath, `${JSON.stringify(record)}\n`, 'utf8')
    } catch {
      // Diagnostics must never take the shell down; stop writing after a
      // filesystem failure instead of retrying on every event.
      this.disabled = true
    }
  }

  get path(): string {
    return this.filePath
  }

  private rotateIfNeeded(): void {
    if (!existsSync(this.filePath) || statSync(this.filePath).size < this.maxBytes) {
      return
    }
    const oldest = `${this.filePath}.${this.maxFiles - 1}`
    if (existsSync(oldest)) {
      rmSync(oldest, { force: true })
    }
    for (let index = this.maxFiles - 2; index >= 1; index -= 1) {
      const source = `${this.filePath}.${index}`
      if (existsSync(source)) {
        renameSync(source, `${this.filePath}.${index + 1}`)
      }
    }
    renameSync(this.filePath, `${this.filePath}.1`)
  }
}
