import { describe, expect, it, vi } from 'vitest'

import { API_NAMESPACE, API_VERSION } from '../src/shared/api'
import { EVENT_CHANNEL, INVOKE_CHANNEL } from '../src/shared/channels'

/**
 * The preload bridge is the entire renderer-visible attack surface, so its
 * shape is pinned by a regression test: adding a member here is a security
 * review, not a refactor (plan 0, stage 2).
 */

const exposed: Array<{ key: string; value: unknown }> = []
const invocations: unknown[][] = []
const listeners = new Map<string, (...args: unknown[]) => void>()
let removed = 0

vi.mock('electron', () => ({
  contextBridge: {
    exposeInMainWorld: (key: string, value: unknown) => exposed.push({ key, value })
  },
  ipcRenderer: {
    invoke: async (...args: unknown[]) => {
      invocations.push(args)
      return { ok: true, value: null }
    },
    on: (channel: string, handler: (...args: unknown[]) => void) =>
      listeners.set(channel, handler),
    removeListener: () => {
      removed += 1
    }
  }
}))

await import('../src/preload/index')

interface ExposedApi {
  readonly v1: Record<string, unknown>
}

function api(): ExposedApi {
  const entry = exposed.at(0)
  expect(entry?.key).toBe(API_NAMESPACE)
  return entry?.value as ExposedApi
}

describe('preload bridge', () => {
  it('exposes exactly one frozen versioned namespace', () => {
    expect(exposed).toHaveLength(1)
    const value = api()
    expect(Object.isFrozen(value)).toBe(true)
    expect(Object.isFrozen(value.v1)).toBe(true)
    expect(Object.keys(value)).toEqual(['v1'])
    expect(value.v1['apiVersion']).toBe(API_VERSION)
  })

  it('exposes only the allow-listed members', () => {
    expect(Object.keys(api().v1).sort()).toEqual([
      'apiVersion',
      'invoke',
      'openExternal',
      'subscribe',
      'writeClipboardText'
    ])
  })

  it('leaks no Electron or Node primitive through the bridge', () => {
    const forbidden = [
      'ipcRenderer',
      'ipcMain',
      'require',
      'process',
      'fs',
      'child_process',
      'shell',
      'webFrame',
      'MessagePort',
      'on',
      'emit',
      'addEventListener'
    ]
    for (const member of forbidden) {
      expect(member in api().v1, `${member} must not be exposed`).toBe(false)
    }
    for (const value of Object.values(api().v1)) {
      expect(typeof value === 'function' || typeof value === 'number').toBe(true)
    }
  })

  it('routes invoke through the single internal channel', async () => {
    const invokeFn = api().v1['invoke'] as (command: string, payload: unknown) => Promise<unknown>
    await invokeFn('shell.getState', {})
    expect(invocations).toEqual([[INVOKE_CHANNEL, 'shell.getState', {}]])
  })

  it('hands back an unsubscribe function instead of an emitter', () => {
    const subscribe = api().v1['subscribe'] as (listener: () => void) => () => void
    const unsubscribe = subscribe(() => {})
    expect(typeof unsubscribe).toBe('function')
    expect(listeners.has(EVENT_CHANNEL)).toBe(true)
    unsubscribe()
    expect(removed).toBe(1)
  })
})
