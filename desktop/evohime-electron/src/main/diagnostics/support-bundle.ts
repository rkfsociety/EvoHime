import { createHash } from 'node:crypto'
import { closeSync, fstatSync, openSync, readSync } from 'node:fs'
import { deflateRawSync } from 'node:zlib'

import { REDACTED, redactText, redactValue, type RedactedValue } from './redact'

export interface SupportBundleFiles {
  readonly manifest: Record<string, unknown>
  readonly health: RedactedValue
  readonly runtime: RedactedValue
  readonly errors: string
  readonly events: string
  readonly logs: string
  readonly issueDraft: string
  readonly redactionReport: Record<string, unknown>
}

const FORBIDDEN = /(?:bearer\s+|sk-|ghp_|gho_|github_pat_|xoxb-)[A-Za-z0-9._+\-/=]+|(?:[A-Za-z]:\\|\\\\\.\\pipe\\)[^\s"'<>|]+/i
const MAX_LOG_LINES_PER_SOURCE = 40
const MAX_LOG_FILES = 4
const MAX_LOG_BYTES = 64 * 1024

/** Claims one live task failure for the automatic support report path. */
export function claimAutomaticSupportReport(
  event: { readonly eventType: string; readonly taskId: string },
  isLive: boolean,
  claimedTaskIds: Set<string>
): boolean {
  if (!isLive || event.eventType !== 'task.failed' || event.taskId.length === 0 || claimedTaskIds.has(event.taskId)) {
    return false
  }
  if (claimedTaskIds.size >= 64) {
    const oldest = claimedTaskIds.values().next().value
    if (typeof oldest === 'string') claimedTaskIds.delete(oldest)
  }
  claimedTaskIds.add(event.taskId)
  return true
}

export function buildSupportBundleFiles(input: {
  readonly snapshot: unknown
  readonly runtime: unknown
  readonly events: readonly { readonly sequenceId: number; readonly eventType: string; readonly payload: string }[]
  readonly logs: readonly string[]
}): SupportBundleFiles {
  const health = redactValue(input.snapshot)
  const runtime = redactValue(input.runtime)
  const events = input.events.slice(0, 200).map((event) => JSON.stringify(redactValue({ sequenceId: event.sequenceId, eventType: event.eventType, payload: redactEventPayload(event.eventType, event.payload) }))).join('\n')
  const errors = input.events.filter((event) => /fail|error|refus/i.test(event.eventType)).slice(0, 32).map((event) => JSON.stringify(redactValue({ eventType: event.eventType, payload: redactEventPayload(event.eventType, event.payload) }))).join('\n')
  const rawLogLines = input.logs
    .slice(0, MAX_LOG_FILES)
    .flatMap((source) => readLogSource(source).slice(-MAX_LOG_LINES_PER_SOURCE))
  const logs = rawLogLines
    .map(redactLogLine)
    .join('\n')
  const observedMarkers = {
    shell_ollama_download_fallback: rawLogLines.some((line) => hasStructuredEvent(line, 'shell.ollama_download_fallback'))
  }
  const issueDraft = [
    '### Problem',
    'EvoHime diagnostic support bundle generated locally.',
    '',
    '### Environment',
    'See runtime.json and health.json; credentials and absolute paths are excluded.',
    '',
    '### Reproduction context',
    'Only bounded event metadata is included.',
    '',
    '### Error classes',
    'See errors.jsonl for normalized event types.',
    '',
    '### Diagnostics',
    'See manifest.json and redaction-report.json.',
    `Ollama fallback event observed: ${observedMarkers.shell_ollama_download_fallback ? 'yes' : 'no'}.`
  ].join('\n')
  const redactionReport = {
    rules_version: 'sensitive-data-guardrails-v1',
    total_matches: 0,
    blocked_sections: ['credentials', 'raw_prompts', 'workspace_files', 'tool_payloads'],
    truncated_sections: input.events.length > 200 ? ['events.jsonl'] : [],
    observed_markers: observedMarkers,
    raw_values_included: false
  }
  const filesWithoutManifest = { 'health.json': health, 'runtime.json': runtime, 'errors.jsonl': errors, 'events.jsonl': events, 'logs.txt': logs, 'issue-draft.md': issueDraft, 'redaction-report.json': redactionReport }
  const manifest = {
    schema: 'evohime-support-bundle-v2',
    included_sections: Object.keys(filesWithoutManifest),
    omissions: ['credentials', 'raw_prompts', 'workspace_files', 'tool_payloads'],
    truncation: redactionReport.truncated_sections,
    file_hashes: Object.fromEntries(Object.entries(filesWithoutManifest).map(([name, value]) => [name, sha256(entryBytes(value))]))
  }
  return { manifest, health, runtime, errors, events, logs, issueDraft, redactionReport }
}

function redactEventPayload(eventType: string, payload: string): RedactedValue {
  try {
    const value = JSON.parse(payload) as unknown
    if (eventType === 'task.failed' && value && typeof value === 'object' && !Array.isArray(value)) {
      const record = value as Record<string, unknown>
      return {
        redacted: true,
        conversation_projection: true,
        terminal: true,
        error_code: safeDiagnosticToken(record.error_code) ?? 'task_failed',
        source: safeDiagnosticToken(record.source ?? record.error_source) ?? 'core',
        operation: safeDiagnosticToken(record.operation ?? record.operation_name ?? record.tool_name) ?? 'task.execute'
      }
    }
    return redactValue(value)
  } catch {
    return REDACTED
  }
}

function safeDiagnosticToken(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const token = value.trim()
  if (!token || [...token].length > 128 || !/^[A-Za-z0-9_.:-]+$/.test(token)) return null
  if (/secret|token|password|bearer|sk-/i.test(token)) return null
  return token
}

function readLogSource(source: string): string[] {
  const pathLike = /[\\/]/.test(source) || /\.jsonl$/i.test(source)
  if (!pathLike) return [source]
  let descriptor: number | undefined
  try {
    descriptor = openSync(source, 'r')
    const size = fstatSync(descriptor).size
    const offset = Math.max(0, size - MAX_LOG_BYTES)
    const buffer = Buffer.alloc(size - offset)
    let read = 0
    while (read < buffer.length) {
      const count = readSync(descriptor, buffer, read, buffer.length - read, offset + read)
      if (count === 0) break
      read += count
    }
    const lines = buffer.subarray(0, read).toString('utf8').split(/\r?\n/).filter(Boolean)
    if (offset > 0) lines.shift()
    return lines
  } catch {
    return []
  } finally {
    if (descriptor !== undefined) closeSync(descriptor)
  }
}

function redactLogLine(line: string): string {
  try {
    return JSON.stringify(redactValue(JSON.parse(line)))
  } catch {
    return redactText(line)
  }
}

function hasStructuredEvent(line: string, eventType: string): boolean {
  try {
    const value = JSON.parse(line) as unknown
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false
    const record = value as Record<string, unknown>
    return record.event === eventType || record.eventType === eventType || record.event_type === eventType
  } catch {
    return false
  }
}

export function serializeSupportBundle(files: SupportBundleFiles): Buffer {
  const entries = {
    'manifest.json': files.manifest,
    'health.json': files.health,
    'runtime.json': files.runtime,
    'errors.jsonl': files.errors,
    'events.jsonl': files.events,
    'logs.txt': files.logs,
    'issue-draft.md': files.issueDraft,
    'redaction-report.json': files.redactionReport
  }
  const contents = Object.entries(entries).map(([name, value]) => [name, entryBytes(value)] as const)
  const allText = contents.map(([, content]) => content.toString('utf8')).join('\n')
  if (FORBIDDEN.test(allText)) throw new Error('support bundle final redaction scan failed')
  return zipArchive(contents)
}

function entryBytes(value: unknown): Buffer {
  return Buffer.from(typeof value === 'string' ? value : JSON.stringify(value), 'utf8')
}

function sha256(value: Buffer): string { return createHash('sha256').update(value).digest('hex') }

function crc32(bytes: Buffer): number {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1))
  }
  return (crc ^ 0xffffffff) >>> 0
}

function zipArchive(entries: readonly (readonly [string, Buffer])[]): Buffer {
  const local: Buffer[] = []
  const central: Buffer[] = []
  let offset = 0
  for (const [name, data] of entries) {
    const nameBytes = Buffer.from(name, 'utf8')
    const compressed = deflateRawSync(data)
    const method = compressed.length < data.length ? 8 : 0
    const stored = method === 8 ? compressed : data
    const header = Buffer.alloc(30)
    header.writeUInt32LE(0x04034b50, 0); header.writeUInt16LE(20, 4); header.writeUInt16LE(0x800, 6); header.writeUInt16LE(method, 8); header.writeUInt32LE(crc32(data), 14); header.writeUInt32LE(stored.length, 18); header.writeUInt32LE(data.length, 22); header.writeUInt16LE(nameBytes.length, 26)
    local.push(header, nameBytes, stored)
    const directory = Buffer.alloc(46)
    directory.writeUInt32LE(0x02014b50, 0); directory.writeUInt16LE(20, 4); directory.writeUInt16LE(20, 6); directory.writeUInt16LE(0x800, 8); directory.writeUInt16LE(method, 10); directory.writeUInt32LE(crc32(data), 16); directory.writeUInt32LE(stored.length, 20); directory.writeUInt32LE(data.length, 24); directory.writeUInt16LE(nameBytes.length, 28); directory.writeUInt32LE(offset, 42)
    central.push(directory, nameBytes)
    offset += header.length + nameBytes.length + stored.length
  }
  const centralBytes = Buffer.concat(central)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10); end.writeUInt32LE(centralBytes.length, 12); end.writeUInt32LE(offset, 16)
  return Buffer.concat([...local, centralBytes, end])
}
