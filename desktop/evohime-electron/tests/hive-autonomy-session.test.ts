import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  rememberHiveAutonomyChoice,
  resetHiveAutonomySessionForTests,
  scheduleHiveAutonomyRestore
} from '../src/main/hive-autonomy-session'
import type { CorePipeClient } from '../src/main/ipc/pipe-client'

type SentCommand = { setHiveSetting: { key: string; value: string } }

const log = vi.fn()

function fakeClient(): { client: CorePipeClient; sent: SentCommand[] } {
  const sent: SentCommand[] = []
  const client = {
    send: vi.fn((command: SentCommand) => {
      sent.push(command)
      return 'queued'
    })
  } as unknown as CorePipeClient
  return { client, sent }
}

describe('hive autonomy session memory', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    resetHiveAutonomySessionForTests()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('restores the enabled choice once after Core reconnects', () => {
    rememberHiveAutonomyChoice('true')
    const { client, sent } = fakeClient()

    scheduleHiveAutonomyRestore(client, log)
    vi.advanceTimersByTime(2_999)
    expect(sent).toEqual([])

    vi.advanceTimersByTime(1)
    expect(sent).toEqual([
      {
        setHiveSetting: {
          schemaVersion: 1,
          requestId: '',
          key: 'proactive_evolution_enabled',
          value: 'true'
        }
      }
    ])
  })

  it('does not restore when autonomy was never enabled in this session', () => {
    rememberHiveAutonomyChoice('false')
    const { client, sent } = fakeClient()

    scheduleHiveAutonomyRestore(client, log)
    vi.advanceTimersByTime(10_000)
    expect(sent).toEqual([])
  })

  it('does not restore on a fresh Eva start without a session choice', () => {
    const { client, sent } = fakeClient()
    scheduleHiveAutonomyRestore(client, log)
    vi.advanceTimersByTime(10_000)
    expect(sent).toEqual([])
  })

  it('debounces repeated reconnects into a single restore', () => {
    rememberHiveAutonomyChoice('true')
    const { client, sent } = fakeClient()

    scheduleHiveAutonomyRestore(client, log)
    vi.advanceTimersByTime(2_000)
    scheduleHiveAutonomyRestore(client, log)
    vi.advanceTimersByTime(3_000)
    expect(sent).toHaveLength(1)
  })

  it('a later manual disable cancels the pending restore', () => {
    rememberHiveAutonomyChoice('true')
    const { client, sent } = fakeClient()
    scheduleHiveAutonomyRestore(client, log)

    rememberHiveAutonomyChoice('false')
    vi.advanceTimersByTime(10_000)
    expect(sent).toEqual([])
  })

  it('ignores values that are not strict boolean strings', () => {
    rememberHiveAutonomyChoice('1')
    const { client, sent } = fakeClient()
    scheduleHiveAutonomyRestore(client, log)
    vi.advanceTimersByTime(10_000)
    expect(sent).toEqual([])
  })
})
