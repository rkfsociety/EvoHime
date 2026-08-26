import type { CoreEvent } from '@shared/api'

export type RecoveryUiState = 'RECOVERING' | 'RESUMABLE' | 'BLOCKED' | 'WAITING_APPROVAL' | 'FAILED' | 'UNKNOWN_OUTCOME'

export interface RecoveryNotice {
  readonly state: RecoveryUiState
  readonly taskId: string
  readonly reason: string
  readonly reasonCode: string
  readonly correlationId: string
  readonly phase?: string | undefined
  readonly canCancel: boolean
  /** Core event this notice was built from; shown in the details view. */
  readonly eventType: string
  readonly sequenceId: number
  /** Already redacted payload of that event, for the details view. */
  readonly details: Record<string, unknown>
}

/**
 * Converts only redacted Core events into the small recovery contract exposed
 * by the shell. The renderer never infers an approval or a retry from prose.
 *
 * `events` arrive newest first (see App), so the first match wins: a stale
 * failure must not outlive the recovery events that came after it.
 */
export function latestRecoveryNotice(events: readonly CoreEvent[]): RecoveryNotice | null {
  for (const event of events) {
    const payload = parsePayload(event.payload)
    const common = {
      taskId: event.taskId,
      canCancel: payloadBoolean(payload, 'can_cancel'),
      reasonCode: stringField(payload, 'reason_code') ?? event.eventType,
      eventType: event.eventType,
      sequenceId: event.sequenceId,
      details: payload
    }
    if (event.eventType === 'storage.progress') {
      return {
        ...common,
        state: 'RECOVERING',
        reason: stringField(payload, 'message') ?? 'Core выполняет восстановление.',
        correlationId: stringField(payload, 'operation_id') ?? event.taskId,
        phase: stringField(payload, 'phase') ?? undefined
      }
    }
    if (event.eventType === 'approval.required') {
      // Approval events are durable and may be replayed after their task was
      // stopped. A terminal task event supersedes every earlier approval for
      // that task; it must not be shown again on the next shell launch.
      if (events.some((candidate) =>
        candidate.taskId === event.taskId &&
        candidate.sequenceId > event.sequenceId &&
        (candidate.eventType === 'task.completed' ||
          candidate.eventType === 'task.failed' ||
          candidate.eventType === 'task.stopped')
      )) {
        continue
      }
      return {
        ...common,
        state: 'WAITING_APPROVAL',
        reason: 'Core ожидает явного подтверждения эффекта.',
        correlationId: stringField(payload, 'approval_id') ?? event.taskId
      }
    }
    if (event.eventType === 'run.recovery.blocked') {
      return {
        ...common,
        state: 'BLOCKED',
        reason: stringField(payload, 'reason') ?? 'Восстановление заблокировано после проверки.',
        correlationId: stringField(payload, 'operation_id') ?? event.taskId
      }
    }
    if (event.eventType === 'run.unknown_outcome' || event.eventType === 'run.recovery.unknown_outcome') {
      return {
        ...common,
        state: 'UNKNOWN_OUTCOME',
        reason: stringField(payload, 'reason') ?? 'Результат операции неизвестен после сбоя; повторный запуск заблокирован.',
        correlationId: stringField(payload, 'operation_id') ?? stringField(payload, 'run_id') ?? event.taskId
      }
    }
    if (event.eventType === 'run.reconciliation.completed') {
      return {
        ...common,
        state: 'RESUMABLE',
        reason: 'Core подтвердил результат после восстановления.',
        correlationId: stringField(payload, 'run_id') ?? event.taskId
      }
    }
    if (event.eventType === 'task.failed') {
      return {
        ...common,
        state: 'FAILED',
        reason: stringField(payload, 'error') ?? 'Операция завершилась ошибкой.',
        correlationId: stringField(payload, 'request_id') ?? event.taskId
      }
    }
  }
  return null
}

function parsePayload(payload: string): Record<string, unknown> {
  try {
  const value: unknown = JSON.parse(payload)
    if (typeof value !== 'object' || value === null) return {}
    const record = value as Record<string, unknown>
    // Core event payloads are externally tagged (for example
    // {"ApprovalRequired": {...}}); accept the flat form too for older logs.
    const keys = Object.keys(record)
    if (keys.length === 1 && keys[0] !== undefined &&
        typeof record[keys[0]] === 'object' && record[keys[0]] !== null) {
      return record[keys[0]] as Record<string, unknown>
    }
    return record
  } catch {
    return {}
  }
}

function stringField(payload: Record<string, unknown>, key: string): string | null {
  const value = payload[key]
  return typeof value === 'string' && value.length > 0 ? value : null
}

function payloadBoolean(payload: Record<string, unknown>, key: string): boolean {
  return payload[key] === true
}
