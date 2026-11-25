import type { CoreEvent } from '@shared/api'

function parsePayloadObject(rawPayload: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(rawPayload) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
    return parsed as Record<string, unknown>
  } catch {
    return null
  }
}

function readApprovalIdFromDecisionEvent(event: CoreEvent): string | null {
  if (
    event.eventType !== 'ledger.approval_decision'
    && event.eventType !== 'approval.decision'
    && event.eventType !== 'approval_decision'
  ) {
    return null
  }
  const parsed = parsePayloadObject(event.payload)
  if (!parsed) return null
  const body = parsed['body']
  if (body && typeof body === 'object' && !Array.isArray(body)) {
    const intentId = (body as Record<string, unknown>)['approval_intent_id']
    if (typeof intentId === 'string' && intentId.length > 0) return intentId
  }
  const direct = parsed['approval_id'] ?? parsed['approval_intent_id']
  return typeof direct === 'string' && direct.length > 0 ? direct : null
}

/**
 * Collects approval IDs already resolved either in the current UI session or
 * durably recorded by Core (`ledger.approval_decision`, `approval.decision`).
 */
export function collectResolvedApprovalIds(
  events: readonly CoreEvent[],
  extraResolvedIds: readonly string[] = []
): ReadonlySet<string> {
  const resolved = new Set<string>()
  for (const id of extraResolvedIds) {
    if (id.length > 0) resolved.add(id)
  }
  for (const event of events) {
    const id = readApprovalIdFromDecisionEvent(event)
    if (id !== null) resolved.add(id)
  }
  return resolved
}

/**
 * Collects task IDs whose interrupted run was permanently blocked during Core
 * startup recovery so stale approvals or typing indicators do not linger.
 */
export function collectRecoveryBlockedTaskIds(events: readonly CoreEvent[]): ReadonlySet<string> {
  const blocked = new Set<string>()
  for (const event of events) {
    if (!event.taskId) continue
    if (event.eventType === 'run.recovery.blocked') {
      blocked.add(event.taskId)
      continue
    }
    if (event.eventType === 'run.recovery.decision') {
      const parsed = parsePayloadObject(event.payload)
      if (parsed?.['decision'] === 'blocked') {
        blocked.add(event.taskId)
      }
    }
  }
  return blocked
}
