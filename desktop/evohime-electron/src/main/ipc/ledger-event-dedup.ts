/**
 * Bounded FIFO de-duplication set for typed execution-ledger `event_id`s
 * (plan 08-3).
 *
 * `event_id` is durable and stable across a Core generation change, unlike
 * `sequence_id` which is only meaningful within one `(core_instance_id,
 * session_epoch)` pair. So this set is capped by size only, oldest id
 * evicted first — it is never cleared on a session-epoch change, otherwise a
 * re-delivered event right after a Core restart would double-emit.
 */

export const DEFAULT_MAX_TRACKED_LEDGER_EVENT_IDS = 4096

export class LedgerEventDedup {
  private readonly seen = new Set<string>()
  private readonly order: string[] = []

  constructor(private readonly maxTracked = DEFAULT_MAX_TRACKED_LEDGER_EVENT_IDS) {}

  /** Returns true the first time this id is observed, false on every repeat. */
  observe(eventId: string): boolean {
    if (this.seen.has(eventId)) {
      return false
    }
    this.seen.add(eventId)
    this.order.push(eventId)
    if (this.order.length > this.maxTracked) {
      const oldest = this.order.shift()
      if (oldest !== undefined) {
        this.seen.delete(oldest)
      }
    }
    return true
  }

  get size(): number {
    return this.seen.size
  }
}
