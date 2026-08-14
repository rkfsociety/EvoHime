import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'

/**
 * Plain-text log of one rebuild.
 *
 * The UI only ever shows the last build line, which is not enough to explain a
 * failure after the fact — a broken download or a compiler error is thousands of
 * lines up. The full output is kept next to the other diagnostics, already
 * redacted by the command runner, bounded in size and truncated on every run so
 * it cannot grow without limit.
 */

export const MAX_BUILD_LOG_BYTES = 4 * 1024 * 1024

export interface BuildLogWriter {
  start(header: string): void
  append(line: string): void
}

export class BuildLog implements BuildLogWriter {
  private written = 0
  private truncated = false

  constructor(
    private readonly file: string,
    private readonly maxBytes: number = MAX_BUILD_LOG_BYTES
  ) {}

  start(header: string): void {
    this.written = 0
    this.truncated = false
    try {
      mkdirSync(dirname(this.file), { recursive: true })
      writeFileSync(this.file, `${header}\n`, 'utf8')
    } catch {
      // Diagnostics must never be the reason an update fails.
    }
  }

  append(line: string): void {
    if (this.truncated) return
    const payload = `${line}\n`
    if (this.written + payload.length > this.maxBytes) {
      this.truncated = true
      this.write('… лог обрезан по размеру\n')
      return
    }
    this.written += payload.length
    this.write(payload)
  }

  private write(payload: string): void {
    try {
      appendFileSync(this.file, payload, 'utf8')
    } catch {
      // Same reason: a log write is never fatal.
    }
  }
}
