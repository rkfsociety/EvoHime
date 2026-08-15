import { beforeEach, describe, expect, it, vi } from 'vitest'

/**
 * Quit policy of the tray, from the updater's point of view.
 *
 * Keep-alive deliberately turns a window close into "hide": the session outlives
 * the window. An update is the one case where that must not apply — the staged
 * package cannot replace files the running shell holds open, and the transaction
 * waits for this process to exit before it touches anything.
 */

type WindowListener = (event: { preventDefault: () => void }) => void

const listeners = new Map<string, WindowListener>()
const quitCalls: string[] = []

vi.mock('electron', () => ({
  app: { quit: () => quitCalls.push('quit') },
  Menu: { buildFromTemplate: (template: unknown) => template },
  Tray: class {
    setContextMenu(): void {}
    setToolTip(): void {}
    on(): void {}
    destroy(): void {}
  }
}))

vi.mock('../src/main/paths', () => ({ resourcePath: (name: string) => name }))
vi.mock('../src/main/window', () => ({ focusWindow: () => undefined }))

const { createTray } = await import('../src/main/tray')

function fakeWindow(): { hidden: boolean } {
  const state = { hidden: false }
  return Object.assign(state, {
    on: (event: string, listener: WindowListener) => listeners.set(event, listener),
    hide: () => {
      state.hidden = true
    }
  })
}

/** Closes the window the way Electron does, reporting whether it was vetoed. */
function close(): boolean {
  let vetoed = false
  listeners.get('close')?.({ preventDefault: () => (vetoed = true) })
  return vetoed
}

describe('tray quit policy', () => {
  beforeEach(() => {
    listeners.clear()
    quitCalls.length = 0
  })

  it('hides the window instead of closing it while keep-alive is on', () => {
    const window = fakeWindow()
    createTray({ window: window as never, log: () => undefined })

    expect(close()).toBe(true)
    expect(window.hidden).toBe(true)
  })

  it('lets the window close once an update asked to quit', () => {
    const window = fakeWindow()
    const tray = createTray({ window: window as never, log: () => undefined })

    tray.forceQuit()

    expect(quitCalls).toEqual(['quit'])
    // Without this the shell survives its own quit, the installation stays
    // locked, and the update is never applied.
    expect(close()).toBe(false)
    expect(window.hidden).toBe(false)
  })
})
