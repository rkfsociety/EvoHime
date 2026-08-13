/**
 * Bounded outbound command queue.
 *
 * Commands are never silently dropped: once the queue is full the sender gets
 * a controlled `queue-full` rejection (plan 0, stage 1). Streaming events are
 * a separate path and may coalesce; commands may not.
 */

export const DEFAULT_MAX_QUEUED_COMMANDS = 256
export const DEFAULT_MAX_QUEUED_BYTES = 8 * 1024 * 1024

export type EnqueueResult = 'queued' | 'queue-full'

export interface QueuedCommand {
  readonly requestId: string
  readonly frame: Uint8Array
}

export class CommandQueue {
  private readonly items: QueuedCommand[] = []
  private queuedBytes = 0
  private rejected = 0

  constructor(
    private readonly maxItems = DEFAULT_MAX_QUEUED_COMMANDS,
    private readonly maxBytes = DEFAULT_MAX_QUEUED_BYTES
  ) {}

  enqueue(command: QueuedCommand): EnqueueResult {
    const size = command.frame.byteLength
    if (this.items.length >= this.maxItems || this.queuedBytes + size > this.maxBytes) {
      this.rejected += 1
      return 'queue-full'
    }
    this.items.push(command)
    this.queuedBytes += size
    return 'queued'
  }

  dequeue(): QueuedCommand | undefined {
    const command = this.items.shift()
    if (command) {
      this.queuedBytes -= command.frame.byteLength
    }
    return command
  }

  /** Drops everything still queued, e.g. after a session epoch change. */
  drain(): QueuedCommand[] {
    const drained = this.items.splice(0, this.items.length)
    this.queuedBytes = 0
    return drained
  }

  get size(): number {
    return this.items.length
  }

  get bytes(): number {
    return this.queuedBytes
  }

  get rejectedCount(): number {
    return this.rejected
  }
}
