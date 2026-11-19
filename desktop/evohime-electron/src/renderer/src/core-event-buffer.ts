import type { CoreEvent } from '@shared/api'

/** Merges a new Core event batch while deduplicating only persisted events. */
export function prependCoreEvents(
  current: readonly CoreEvent[],
  incomingNewestFirst: readonly CoreEvent[],
  limit: number
): CoreEvent[] {
  const seen = new Set<string>()
  return [...incomingNewestFirst, ...current].filter((event) => {
    // Command responses (including model catalogs) use sequence_id=0 and are
    // transient. Repeated responses of one type are distinct request results,
    // not replayed copies of the same persisted event.
    if (event.sequenceId === 0) return true

    const key = `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${event.sequenceId}:${event.taskId}:${event.eventType}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  }).slice(0, limit)
}
