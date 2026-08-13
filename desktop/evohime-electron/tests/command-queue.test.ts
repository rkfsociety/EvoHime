import { describe, expect, it } from 'vitest'

import { backoffDelayMs } from '../src/main/ipc/backoff'
import { CommandQueue } from '../src/main/ipc/command-queue'

const command = (requestId: string, size = 8): { requestId: string; frame: Uint8Array } => ({
  requestId,
  frame: new Uint8Array(size)
})

describe('command queue', () => {
  it('rejects instead of dropping when the item limit is reached', () => {
    const queue = new CommandQueue(2)
    expect(queue.enqueue(command('a'))).toBe('queued')
    expect(queue.enqueue(command('b'))).toBe('queued')
    expect(queue.enqueue(command('c'))).toBe('queue-full')
    expect(queue.rejectedCount).toBe(1)
    // The already accepted commands survive the rejection.
    expect(queue.dequeue()?.requestId).toBe('a')
    expect(queue.dequeue()?.requestId).toBe('b')
    expect(queue.dequeue()).toBeUndefined()
  })

  it('rejects when the byte budget is exceeded', () => {
    const queue = new CommandQueue(10, 16)
    expect(queue.enqueue(command('a', 16))).toBe('queued')
    expect(queue.enqueue(command('b', 1))).toBe('queue-full')
    expect(queue.bytes).toBe(16)
  })

  it('drains every queued command at once', () => {
    const queue = new CommandQueue()
    queue.enqueue(command('a'))
    queue.enqueue(command('b'))
    expect(queue.drain()).toHaveLength(2)
    expect(queue.size).toBe(0)
    expect(queue.bytes).toBe(0)
  })
})

describe('reconnect backoff', () => {
  it('grows exponentially and stays bounded', () => {
    const options = { baseMs: 100, maxMs: 1_000, jitterRatio: 0 }
    expect(backoffDelayMs(0, options)).toBe(100)
    expect(backoffDelayMs(1, options)).toBe(200)
    expect(backoffDelayMs(2, options)).toBe(400)
    expect(backoffDelayMs(50, options)).toBe(1_000)
  })

  it('never exceeds the maximum even with full jitter', () => {
    const options = { baseMs: 100, maxMs: 1_000, jitterRatio: 0.5 }
    expect(backoffDelayMs(3, options, () => 1)).toBe(1_000)
    expect(backoffDelayMs(0, options, () => 1)).toBe(150)
  })
})
