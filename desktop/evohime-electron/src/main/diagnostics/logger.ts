import { appendFile, mkdir, rename, rm, stat } from 'node:fs/promises'
import { join } from 'node:path'

import { redactText, redactValue, type RedactedValue } from './redact'

/**
 * Redacted JSONL diagnostics for one Electron process.
 *
 * Writes are serialized asynchronously and the pending queue is bounded by
 * both record count and encoded bytes. Core remains authoritative for agent
 * events; shell diagnostics are best-effort and may lose the bounded pending
 * tail if the process crashes or the filesystem fails.
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error'

/** Signature every component uses to emit a redacted shell diagnostic. */
export type ShellLog = (
  level: LogLevel,
  event: string,
  fields?: Record<string, unknown>
) => void

export type ShellLogStream = 'main' | 'renderer' | 'updater'

export interface LoggerFileSystem {
  mkdir(path: string, options: { recursive: true }): Promise<unknown>
  stat(path: string): Promise<{ readonly size: number }>
  appendFile(path: string, data: string, encoding: 'utf8'): Promise<void>
  rename(oldPath: string, newPath: string): Promise<void>
  rm(path: string, options: { force: true }): Promise<void>
}

export interface JsonlLoggerOptions {
  readonly directory: string
  readonly stream: ShellLogStream
  readonly maxBytes?: number
  readonly maxFiles?: number
  readonly maxQueueRecords?: number
  readonly maxQueueBytes?: number
  /** Injectable for tests; production passes the real clock. */
  readonly now?: () => Date
  /** Injectable async file operations keep slow and failing I/O deterministic in tests. */
  readonly fileSystem?: LoggerFileSystem
}

export interface JsonlLoggerStatus {
  readonly disabled: boolean
  readonly droppedRecords: number
  readonly queuedRecords: number
  readonly queuedBytes: number
}

export const DEFAULT_MAX_LOG_BYTES = 4 * 1024 * 1024
export const DEFAULT_MAX_LOG_FILES = 3
export const DEFAULT_MAX_QUEUE_RECORDS = 512
export const DEFAULT_MAX_QUEUE_BYTES = 1024 * 1024
const DEFAULT_FLUSH_TIMEOUT_MS = 1_500

const defaultFileSystem: LoggerFileSystem = { mkdir, stat, appendFile, rename, rm }

export class JsonlLogger {
  private readonly filePath: string
  private readonly maxBytes: number
  private readonly maxFiles: number
  private readonly maxQueueRecords: number
  private readonly maxQueueBytes: number
  private readonly now: () => Date
  private readonly fileSystem: LoggerFileSystem
  private readonly queue: string[] = []
  private queuedBytes = 0
  private droppedRecords = 0
  private disabled = false
  private currentFileBytes: number | null = null
  private initializationPromise: Promise<void> | null = null
  private drainPromise: Promise<void> | null = null

  constructor(private readonly options: JsonlLoggerOptions) {
    this.filePath = join(options.directory, `shell-${options.stream}.jsonl`)
    this.maxBytes = Math.max(1, options.maxBytes ?? DEFAULT_MAX_LOG_BYTES)
    this.maxFiles = Math.max(1, Math.floor(options.maxFiles ?? DEFAULT_MAX_LOG_FILES))
    this.maxQueueRecords = Math.max(1, Math.floor(options.maxQueueRecords ?? DEFAULT_MAX_QUEUE_RECORDS))
    this.maxQueueBytes = Math.max(1, options.maxQueueBytes ?? DEFAULT_MAX_QUEUE_BYTES)
    this.now = options.now ?? (() => new Date())
    this.fileSystem = options.fileSystem ?? defaultFileSystem
  }

  write(level: LogLevel, event: string, fields: Record<string, unknown> = {}): void {
    if (this.disabled) {
      this.droppedRecords += 1
      return
    }
    let line: string
    try {
      const record: Record<string, RedactedValue> = {
        ts: this.now().toISOString(),
        level,
        stream: this.options.stream,
        event: redactText(event),
        ...(redactValue(fields) as Record<string, RedactedValue>)
      }
      line = `${JSON.stringify(record)}\n`
    } catch {
      this.disabled = true
      this.droppedRecords += this.queue.length + 1
      this.queue.length = 0
      this.queuedBytes = 0
      return
    }
    const lineBytes = Buffer.byteLength(line, 'utf8')
    if (this.queue.length >= this.maxQueueRecords || this.queuedBytes + lineBytes > this.maxQueueBytes) {
      this.droppedRecords += 1
      return
    }
    this.queue.push(line)
    this.queuedBytes += lineBytes
    this.startDrain()
  }

  get path(): string {
    return this.filePath
  }

  get status(): JsonlLoggerStatus {
    return {
      disabled: this.disabled,
      droppedRecords: this.droppedRecords,
      queuedRecords: this.queue.length,
      queuedBytes: this.queuedBytes
    }
  }

  /** Waits for queued writes up to a bounded deadline; filesystem errors never escape. */
  async flush(timeoutMs = DEFAULT_FLUSH_TIMEOUT_MS): Promise<boolean> {
    if (this.disabled) return false
    if (this.queue.length === 0 && this.drainPromise === null) return true
    const drain = this.startDrain()
    let timer: NodeJS.Timeout | undefined
    try {
      const completed = await Promise.race([
        drain.then(() => true),
        new Promise<boolean>((resolve) => {
          timer = setTimeout(() => resolve(false), Math.max(0, timeoutMs))
          timer.unref?.()
        })
      ])
      return completed && !this.disabled && this.queue.length === 0
    } catch {
      return false
    } finally {
      if (timer) clearTimeout(timer)
    }
  }

  private startDrain(): Promise<void> {
    if (this.drainPromise) return this.drainPromise
    const draining = this.drain()
    this.drainPromise = draining.finally(() => {
      this.drainPromise = null
      if (this.queue.length > 0 && !this.disabled) this.startDrain()
    })
    return this.drainPromise
  }

  private async drain(): Promise<void> {
    try {
      await this.initialize()
      while (this.queue.length > 0 && !this.disabled) {
        const line = this.queue[0]
        if (line === undefined) break
        if ((this.currentFileBytes ?? 0) >= this.maxBytes) await this.rotate()
        await this.fileSystem.appendFile(this.filePath, line, 'utf8')
        this.currentFileBytes = (this.currentFileBytes ?? 0) + Buffer.byteLength(line, 'utf8')
        this.queue.shift()
        this.queuedBytes -= Buffer.byteLength(line, 'utf8')
      }
    } catch {
      // Diagnostics must never take the shell down or recursively log their
      // own failure. Drop the bounded tail and expose only a count in status.
      this.disabled = true
      this.droppedRecords += this.queue.length
      this.queue.length = 0
      this.queuedBytes = 0
    }
  }

  private initialize(): Promise<void> {
    if (this.initializationPromise) return this.initializationPromise
    this.initializationPromise = (async () => {
      await this.fileSystem.mkdir(this.options.directory, { recursive: true })
      try {
        this.currentFileBytes = (await this.fileSystem.stat(this.filePath)).size
      } catch (error) {
        if (!isMissingFile(error)) throw error
        this.currentFileBytes = 0
      }
    })()
    return this.initializationPromise
  }

  private async rotate(): Promise<void> {
    if (this.maxFiles === 1) {
      await this.fileSystem.rm(this.filePath, { force: true })
      this.currentFileBytes = 0
      return
    }
    const oldest = `${this.filePath}.${this.maxFiles - 1}`
    await this.fileSystem.rm(oldest, { force: true })
    for (let index = this.maxFiles - 2; index >= 1; index -= 1) {
      await renameIfPresent(this.fileSystem, `${this.filePath}.${index}`, `${this.filePath}.${index + 1}`)
    }
    await this.fileSystem.rename(this.filePath, `${this.filePath}.1`)
    this.currentFileBytes = 0
  }
}

async function renameIfPresent(fileSystem: LoggerFileSystem, source: string, destination: string): Promise<void> {
  try {
    await fileSystem.rename(source, destination)
  } catch (error) {
    if (!isMissingFile(error)) throw error
  }
}

function isMissingFile(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT'
}
