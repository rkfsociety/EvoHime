import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { CommandFailure, RendererCommand, ShellState } from '../src/shared/api'
import {
  CLIPBOARD_CHANNEL,
  INVOKE_CHANNEL,
  OPEN_EXTERNAL_CHANNEL
} from '../src/shared/channels'

/**
 * Regression tests for the only path a renderer has into Core.
 *
 * The bridge must forward exactly the allow-listed commands with validated
 * payloads and refuse everything else. Core still re-validates capability,
 * policy and approval — these tests cover the shell's own narrow surface.
 */

type Handler = (event: unknown, ...args: unknown[]) => unknown

const handlers = new Map<string, Handler>()
const clipboardWrites: string[] = []
const openedUrls: string[] = []

vi.mock('electron', () => ({
  ipcMain: {
    handle: (channel: string, handler: Handler) => handlers.set(channel, handler)
  },
  clipboard: {
    writeText: (text: string) => clipboardWrites.push(text)
  },
  shell: {
    openExternal: async (url: string) => {
      openedUrls.push(url)
    }
  },
  BrowserWindow: {
    getAllWindows: () => []
  }
}))

const { registerShellBridge } = await import('../src/main/shell-bridge')

interface SentCommand {
  readonly [key: string]: unknown
}

const sent: SentCommand[] = []
let enqueueResult: 'queued' | 'queue-full' = 'queued'

const fakeState: ShellState = {
  connection: 'connected',
  protocol: { major: 1, minor: 0 },
  capabilities: ['replay'],
  coreVersion: '0.1.0',
  lastSequence: 4,
  reason: null,
  reconnectAttempts: 0
}

const client = {
  get state(): ShellState {
    return fakeState
  },
  send(command: SentCommand): 'queued' | 'queue-full' {
    sent.push(command)
    return enqueueResult
  },
  requestResync(): 'queued' | 'queue-full' {
    sent.push({ resyncRequest: {} })
    return enqueueResult
  }
}

function invoke(command: string, payload?: unknown): unknown {
  const handler = handlers.get(INVOKE_CHANNEL)
  if (!handler) {
    throw new Error('invoke channel is not registered')
  }
  return handler({}, command, payload)
}

beforeEach(() => {
  handlers.clear()
  sent.length = 0
  clipboardWrites.length = 0
  openedUrls.length = 0
  enqueueResult = 'queued'
  registerShellBridge({
    client: client as never,
    log: () => {}
  })
})

describe('renderer command surface', () => {
  it('exposes only the documented channels', () => {
    expect([...handlers.keys()].sort()).toEqual(
      [CLIPBOARD_CHANNEL, INVOKE_CHANNEL, OPEN_EXTERNAL_CHANNEL].sort()
    )
  })

  it('returns the shell state without touching Core', () => {
    expect(invoke('shell.getState', {})).toEqual({ ok: true, value: fakeState })
    expect(sent).toHaveLength(0)
  })

  it('forwards an allow-listed command', () => {
    const outcome = invoke('core.startTask', {
      taskId: 'task-1',
      prompt: 'сделай',
      workspacePath: 'C:\\work'
    })
    expect(outcome).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toEqual([
      { startTask: { taskId: 'task-1', prompt: 'сделай', workspacePath: 'C:\\work' } }
    ])
  })

  it('rejects a command outside the allow-list', () => {
    for (const command of ['core.deleteEverything', 'shell.exec', '__proto__', '']) {
      const outcome = invoke(command, {}) as CommandFailure
      expect(outcome.ok).toBe(false)
      expect(outcome.code).toBe('unknown-command')
    }
    expect(sent).toHaveLength(0)
  })

  it('rejects malformed payloads instead of forwarding them', () => {
    const cases: Array<[RendererCommand, unknown]> = [
      ['core.startTask', { taskId: 'task-1' }],
      ['core.startTask', { taskId: '', prompt: 'p', workspacePath: 'C:\\w' }],
      ['core.startTask', null],
      ['core.stopTask', { taskId: 42 }],
      ['core.resolveApproval', { approvalId: 'a-1' }],
      ['core.resolveApproval', { approvalId: 'a-1', granted: 'yes' }]
    ]
    for (const [command, payload] of cases) {
      const outcome = invoke(command, payload) as CommandFailure
      expect(outcome.ok, `${command} ${JSON.stringify(payload)}`).toBe(false)
      expect(outcome.code).toBe('invalid-payload')
    }
    expect(sent).toHaveLength(0)
  })

  it('bounds oversized text fields', () => {
    const outcome = invoke('core.stopTask', { taskId: 'x'.repeat(5_000) }) as CommandFailure
    expect(outcome.code).toBe('invalid-payload')
    expect(sent).toHaveLength(0)
  })

  it('surfaces a full queue as a typed failure', () => {
    enqueueResult = 'queue-full'
    const outcome = invoke('core.stopTask', { taskId: 'task-1' }) as CommandFailure
    expect(outcome.ok).toBe(false)
    expect(outcome.code).toBe('queue-full')
  })
})

describe('clipboard and external links', () => {
  it('writes bounded plain text and never reads the clipboard back', async () => {
    const handler = handlers.get(CLIPBOARD_CHANNEL)!
    expect(await handler({}, 'скопируй меня')).toBe(true)
    expect(clipboardWrites).toEqual(['скопируй меня'])

    expect(await handler({}, '')).toBe(false)
    expect(await handler({}, 'x'.repeat(70_000))).toBe(false)
    expect(await handler({}, 42)).toBe(false)
    expect(clipboardWrites).toHaveLength(1)
  })

  it('opens only allow-listed https URLs', async () => {
    const handler = handlers.get(OPEN_EXTERNAL_CHANNEL)!
    expect(await handler({}, 'https://github.com/evohime')).toBe(true)
    expect(await handler({}, 'https://evil.tld/')).toBe(false)
    expect(await handler({}, 'file:///C:/Windows/System32/cmd.exe')).toBe(false)
    expect(await handler({}, 'javascript:alert(1)')).toBe(false)
    expect(openedUrls).toEqual(['https://github.com/evohime'])
  })
})
