import { createHash } from 'node:crypto'

import { redactText, redactValue, type RedactedValue } from './redact'

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

export function buildSupportBundleFiles(input: {
  readonly snapshot: unknown
  readonly runtime: unknown
  readonly events: readonly { readonly sequenceId: number; readonly eventType: string; readonly payload: string }[]
  readonly logs: readonly string[]
}): SupportBundleFiles {
  const health = redactValue(input.snapshot)
  const runtime = redactValue(input.runtime)
  const events = input.events.slice(0, 200).map((event) => JSON.stringify(redactValue({ sequenceId: event.sequenceId, eventType: event.eventType, payload: event.payload }))).join('\n')
  const errors = input.events.filter((event) => /fail|error|refus/i.test(event.eventType)).slice(0, 32).map((event) => JSON.stringify(redactValue({ eventType: event.eventType, payload: event.payload }))).join('\n')
  const logs = input.logs.slice(0, 120).map(redactText).join('\n')
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
    'See manifest.json and redaction-report.json.'
  ].join('\n')
  const redactionReport = {
    rules_version: 'sensitive-data-guardrails-v1',
    total_matches: 0,
    blocked_sections: ['credentials', 'raw_prompts', 'workspace_files', 'tool_payloads'],
    truncated_sections: input.events.length > 200 ? ['events.jsonl'] : [],
    raw_values_included: false
  }
  const filesWithoutManifest = { 'health.json': health, 'runtime.json': runtime, 'errors.jsonl': errors, 'events.jsonl': events, 'logs.txt': logs, 'issue-draft.md': issueDraft, 'redaction-report.json': redactionReport }
  const manifest = {
    schema: 'evohime-support-bundle-v2',
    included_sections: Object.keys(filesWithoutManifest),
    omissions: ['credentials', 'raw_prompts', 'workspace_files', 'tool_payloads'],
    truncation: redactionReport.truncated_sections,
    file_hashes: Object.fromEntries(Object.entries(filesWithoutManifest).map(([name, value]) => [name, sha256(JSON.stringify(value))]))
  }
  return { manifest, health, runtime, errors, events, logs, issueDraft, redactionReport }
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
  const contents = Object.entries(entries).map(([name, value]) => [name, Buffer.from(typeof value === 'string' ? value : JSON.stringify(value), 'utf8')] as const)
  const allText = contents.map(([, content]) => content.toString('utf8')).join('\n')
  if (FORBIDDEN.test(allText)) throw new Error('support bundle final redaction scan failed')
  return zipStore(contents)
}

function sha256(value: string): string { return createHash('sha256').update(value, 'utf8').digest('hex') }

function crc32(bytes: Buffer): number {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1))
  }
  return (crc ^ 0xffffffff) >>> 0
}

function zipStore(entries: readonly (readonly [string, Buffer])[]): Buffer {
  const local: Buffer[] = []
  const central: Buffer[] = []
  let offset = 0
  for (const [name, data] of entries) {
    const nameBytes = Buffer.from(name, 'utf8')
    const header = Buffer.alloc(30)
    header.writeUInt32LE(0x04034b50, 0); header.writeUInt16LE(20, 4); header.writeUInt16LE(0x800, 6); header.writeUInt32LE(crc32(data), 14); header.writeUInt32LE(data.length, 18); header.writeUInt32LE(data.length, 22); header.writeUInt16LE(nameBytes.length, 26)
    local.push(header, nameBytes, data)
    const directory = Buffer.alloc(46)
    directory.writeUInt32LE(0x02014b50, 0); directory.writeUInt16LE(20, 4); directory.writeUInt16LE(20, 6); directory.writeUInt16LE(0x800, 8); directory.writeUInt32LE(crc32(data), 16); directory.writeUInt32LE(data.length, 20); directory.writeUInt32LE(data.length, 24); directory.writeUInt16LE(nameBytes.length, 28); directory.writeUInt32LE(offset, 42)
    central.push(directory, nameBytes)
    offset += header.length + nameBytes.length + data.length
  }
  const centralBytes = Buffer.concat(central)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10); end.writeUInt32LE(centralBytes.length, 12); end.writeUInt32LE(offset, 16)
  return Buffer.concat([...local, centralBytes, end])
}
