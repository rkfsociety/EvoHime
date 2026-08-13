import type { CoreEvent } from '@shared/api'

/**
 * Turns the raw Core event stream into what a person actually wants to read.
 *
 * One line per event is noise. An answer arrives as many deltas, and a burst
 * of tool calls is one stretch of work, not a dozen entries — so deltas merge
 * into a single message and consecutive tool calls collapse into one activity
 * line that updates in place. Text from the model ends the current stretch, so
 * the transcript reads as: work → answer → work → answer.
 */

const MAX_TEXT_CHARS = 8_192
/** Bookkeeping the user has no use for: the prompt is already on screen. */
const SILENT_EVENTS = new Set(['task.started', 'model.context', 'storage.progress'])

export interface ToolCall {
  readonly tool: string
  readonly output: string | null
  /** True until the matching output arrives. */
  readonly running: boolean
}

export type TranscriptEntry =
  | { readonly kind: 'agent'; readonly id: string; readonly text: string }
  | {
      readonly kind: 'activity'
      readonly id: string
      readonly calls: readonly ToolCall[]
      /** True while the last call of the stretch has no output yet. */
      readonly running: boolean
    }
  | { readonly kind: 'result'; readonly id: string; readonly text: string; readonly failed: boolean }
  | { readonly kind: 'stopped'; readonly id: string }

export interface Approval {
  readonly approvalId: string
  readonly toolName: string
  readonly permission: string
  readonly scope: string
}

export interface Transcript {
  readonly entries: readonly TranscriptEntry[]
  readonly approval: Approval | null
  /** True once the task reached a terminal event. */
  readonly finished: boolean
}

/** `events` is newest-first, the way the shell keeps them. */
export function buildTranscript(events: readonly CoreEvent[]): Transcript {
  const entries: TranscriptEntry[] = []
  let approval: Approval | null = null
  let finished = false

  for (const event of [...events].reverse()) {
    if (SILENT_EVENTS.has(event.eventType)) continue
    const payload = unwrap(event.payload)
    const id = String(event.sequenceId)

    switch (event.eventType) {
      case 'agent.message.delta': {
        const text = text_(payload, 'content')
        if (text.length === 0) break
        // Deltas are fragments of one message, not separate messages.
        const last = entries.at(-1)
        if (last?.kind === 'agent') {
          entries[entries.length - 1] = { ...last, text: clamp(last.text + text) }
        } else {
          entries.push({ kind: 'agent', id, text: clamp(text) })
        }
        break
      }

      case 'tool.started': {
        const tool = text_(payload, 'tool_name') || 'инструмент'
        const call: ToolCall = { tool, output: null, running: true }
        const last = entries.at(-1)
        // Consecutive calls belong to the same stretch of work.
        if (last?.kind === 'activity') {
          entries[entries.length - 1] = { ...last, calls: [...last.calls, call], running: true }
        } else {
          entries.push({ kind: 'activity', id, calls: [call], running: true })
        }
        break
      }

      case 'tool.output': {
        const tool = text_(payload, 'tool_name') || 'инструмент'
        const output = clamp(text_(payload, 'output'))
        const index = findLastIndex(entries, (entry) => entry.kind === 'activity')
        const group = index >= 0 ? (entries[index] as Extract<TranscriptEntry, { kind: 'activity' }>) : null
        if (!group) {
          entries.push({
            kind: 'activity',
            id,
            calls: [{ tool, output, running: false }],
            running: false
          })
          break
        }
        const callIndex = findLastCall(group.calls, tool)
        const calls =
          callIndex >= 0
            ? group.calls.map((call, position) =>
                position === callIndex ? { ...call, output, running: false } : call
              )
            : [...group.calls, { tool, output, running: false }]
        entries[index] = { ...group, calls, running: calls.some((call) => call.running) }
        break
      }

      case 'approval.required': {
        const approvalId = text_(payload, 'approval_id')
        if (approvalId.length > 0) {
          approval = {
            approvalId,
            toolName: text_(payload, 'tool_name') || 'операция Core',
            permission: text_(payload, 'permission') || 'требуется разрешение',
            scope: text_(payload, 'scope') || 'область не указана'
          }
        }
        break
      }

      case 'task.completed': {
        finished = true
        const text = clamp(text_(payload, 'final_message'))
        // An empty completion adds nothing over the answer already shown.
        if (text.length > 0) entries.push({ kind: 'result', id, text, failed: false })
        break
      }

      case 'task.failed': {
        finished = true
        entries.push({
          kind: 'result',
          id,
          text: clamp(text_(payload, 'error')) || 'Задача завершилась ошибкой.',
          failed: true
        })
        break
      }

      case 'task.stopped': {
        finished = true
        entries.push({ kind: 'stopped', id })
        break
      }

      default:
        break
    }
  }

  // A finished task leaves nothing pending, so a stale prompt must not linger.
  if (finished) approval = null

  return { entries, approval, finished }
}

/**
 * Core serialises `CoreEvent` as an externally tagged enum, so the fields live
 * one level down under the variant name.
 */
function unwrap(payload: string): Record<string, unknown> {
  if (payload.length === 0 || payload.length > MAX_TEXT_CHARS * 4) return {}
  let parsed: unknown
  try {
    parsed = JSON.parse(payload)
  } catch {
    return {}
  }
  if (typeof parsed !== 'object' || parsed === null) return {}
  const record = parsed as Record<string, unknown>
  if (typeof record['task_id'] === 'string') return record
  const keys = Object.keys(record)
  const inner = keys.length === 1 && keys[0] !== undefined ? record[keys[0]] : null
  return typeof inner === 'object' && inner !== null ? (inner as Record<string, unknown>) : record
}

function text_(payload: Record<string, unknown>, key: string): string {
  const value = payload[key]
  return typeof value === 'string' ? value : ''
}

function clamp(value: string): string {
  return value.length > MAX_TEXT_CHARS ? `${value.slice(0, MAX_TEXT_CHARS)}…` : value
}

function findLastIndex(
  entries: readonly TranscriptEntry[],
  predicate: (entry: TranscriptEntry) => boolean
): number {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index]
    if (entry !== undefined && predicate(entry)) return index
  }
  return -1
}

function findLastCall(calls: readonly ToolCall[], tool: string): number {
  for (let index = calls.length - 1; index >= 0; index -= 1) {
    const call = calls[index]
    if (call !== undefined && call.running && call.tool === tool) return index
  }
  return -1
}
