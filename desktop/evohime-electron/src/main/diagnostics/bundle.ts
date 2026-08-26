import { closeSync, fstatSync, openSync, readSync } from 'node:fs'

import { redactText, redactValue, type RedactedValue } from './redact'

const MAX_EVENTS = 200
const MAX_LOG_LINES = 120
const MAX_LOG_FILES = 4
const MAX_LOG_BYTES = 64 * 1024
const MAX_BUNDLE_BYTES = 512 * 1024

export interface DiagnosticBundleInput {
  readonly generatedAtMs: number
  readonly appVersion: string
  readonly platform: string
  readonly architecture: string
  readonly state: unknown
  readonly update: unknown
  readonly repair: unknown
  readonly events: readonly { sequenceId: number; taskId: string; eventType: string; payload: string }[]
  readonly logPaths: readonly string[]
}

export interface DiagnosticBundle {
  readonly schema: 'evohime-diagnostic-bundle-v1'
  readonly generatedAtMs: number
  readonly runtime: { readonly appVersion: string; readonly platform: string; readonly architecture: string }
  readonly state: RedactedValue
  readonly update: RedactedValue
  readonly repair: RedactedValue
  readonly events: readonly RedactedValue[]
  readonly logExcerpts: readonly RedactedValue[]
}

/** Builds a bounded, redacted support artifact. It never reads workspace files. */
export function buildDiagnosticBundle(input: DiagnosticBundleInput): DiagnosticBundle {
  const events = input.events.slice(0, MAX_EVENTS).map((event) => redactValue({
    sequenceId: event.sequenceId,
    taskId: event.taskId,
    eventType: event.eventType,
    payload: redactPayload(event.payload)
  }))
  const logExcerpts: RedactedValue[] = []
  for (const path of input.logPaths.slice(0, MAX_LOG_FILES)) {
    const remaining = MAX_LOG_LINES - logExcerpts.length
    if (remaining <= 0) break
    for (const line of readLogTail(path, remaining)) {
      logExcerpts.push(redactText(line))
    }
  }
  const bundle: DiagnosticBundle = {
    schema: 'evohime-diagnostic-bundle-v1',
    generatedAtMs: input.generatedAtMs,
    runtime: {
      appVersion: redactText(input.appVersion).slice(0, 128),
      platform: redactText(input.platform).slice(0, 64),
      architecture: redactText(input.architecture).slice(0, 64)
    },
    state: redactValue(input.state),
    update: redactValue(input.update),
    repair: redactValue(input.repair),
    events,
    logExcerpts
  }
  return bundle
}

/** Reads only a bounded tail; an untrusted log cannot force a whole-file read. */
function readLogTail(path: string, maxLines: number): string[] {
  let descriptor: number | undefined
  try {
    descriptor = openSync(path, 'r')
    const size = fstatSync(descriptor).size
    const offset = Math.max(0, size - MAX_LOG_BYTES)
    const buffer = Buffer.alloc(size - offset)
    let read = 0
    while (read < buffer.length) {
      const count = readSync(descriptor, buffer, read, buffer.length - read, offset + read)
      if (count === 0) break
      read += count
    }
    const text = buffer.subarray(0, read).toString('utf8')
    const lines = text.split(/\r?\n/).filter(Boolean)
    if (offset > 0) lines.shift()
    return lines.slice(-maxLines)
  } catch {
    return []
  } finally {
    if (descriptor !== undefined) closeSync(descriptor)
  }
}

function redactPayload(payload: string): RedactedValue {
  try {
    return redactValue(JSON.parse(payload))
  } catch {
    return redactText(payload)
  }
}

export function serializeDiagnosticBundle(bundle: DiagnosticBundle): string {
  const serialized = JSON.stringify(bundle)
  if (Buffer.byteLength(serialized, 'utf8') > MAX_BUNDLE_BYTES) {
    throw new Error('diagnostic bundle exceeds bounded size')
  }
  return `${serialized}\n`
}
