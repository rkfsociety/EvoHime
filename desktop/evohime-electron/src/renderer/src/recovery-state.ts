import type { CoreEvent } from '@shared/api'

export type RecoveryUiState = 'RECOVERING' | 'BLOCKED' | 'WAITING_APPROVAL' | 'FAILED'

export interface RecoveryNotice {
  readonly state: RecoveryUiState
  readonly taskId: string
  readonly reason: string
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
      canCancel: false,
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
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
  } catch {
    return {}
  }
}

function stringField(payload: Record<string, unknown>, key: string): string | null {
  const value = payload[key]
  return typeof value === 'string' && value.length > 0 ? value : null
}
