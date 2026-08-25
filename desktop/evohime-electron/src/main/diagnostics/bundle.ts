import { readFileSync } from 'node:fs'

import { redactText, redactValue, type RedactedValue } from './redact'

const MAX_EVENTS = 200
const MAX_LOG_LINES = 120
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
  const logExcerpts = input.logPaths.flatMap((path) => {
    try {
      return readFileSync(path, 'utf8').split(/\r?\n/).filter(Boolean).slice(-MAX_LOG_LINES).map((line) => redactText(line))
    } catch {
      return []
    }
  })
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
    logExcerpts: logExcerpts.slice(-MAX_LOG_LINES)
  }
  return bundle
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
