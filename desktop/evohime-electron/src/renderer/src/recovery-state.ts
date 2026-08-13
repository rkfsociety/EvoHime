import type { CoreEvent } from '@shared/api'

export type RecoveryUiState = 'RECOVERING' | 'BLOCKED' | 'WAITING_APPROVAL' | 'FAILED'

export interface RecoveryNotice {
  readonly state: RecoveryUiState
  readonly taskId: string
  readonly reason: string
  readonly correlationId: string
  readonly phase?: string | undefined
  readonly canCancel: boolean
}

/**
 * Converts only redacted Core events into the small recovery contract exposed
 * by the shell. The renderer never infers an approval or a retry from prose.
 */
export function latestRecoveryNotice(events: readonly CoreEvent[]): RecoveryNotice | null {
  for (const event of [...events].reverse()) {
    const payload = parsePayload(event.payload)
    if (event.eventType === 'storage.progress') {
      return {
        state: 'RECOVERING',
        taskId: event.taskId,
        reason: stringField(payload, 'message') ?? 'Core выполняет восстановление.',
        correlationId: stringField(payload, 'operation_id') ?? event.taskId,
        phase: stringField(payload, 'phase') ?? undefined,
        canCancel: false
      }
    }
    if (event.eventType === 'approval.required') {
      return {
        state: 'WAITING_APPROVAL',
        taskId: event.taskId,
        reason: 'Core ожидает явного подтверждения эффекта.',
        correlationId: stringField(payload, 'approval_id') ?? event.taskId,
        canCancel: false
      }
    }
    if (event.eventType === 'run.recovery.blocked') {
      return {
        state: 'BLOCKED',
        taskId: event.taskId,
        reason: stringField(payload, 'reason') ?? 'Восстановление заблокировано после проверки.',
        correlationId: stringField(payload, 'operation_id') ?? event.taskId,
        canCancel: false
      }
    }
    if (event.eventType === 'task.failed') {
      return {
        state: 'FAILED',
        taskId: event.taskId,
        reason: stringField(payload, 'error') ?? 'Операция завершилась ошибкой.',
        correlationId: stringField(payload, 'request_id') ?? event.taskId,
        canCancel: false
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
