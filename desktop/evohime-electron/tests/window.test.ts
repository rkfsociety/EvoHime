import { describe, expect, it, vi } from 'vitest'

const fakeElectron = vi.hoisted(() => {
  class FakeBrowserWindow {
    readonly webContents = { on: (): void => undefined }
    readonly actions: string[] = []
    private readonly listeners = new Map<string, () => void>()
    private minimized = false
    private visible = false
    private destroyed = false

    readonly options: unknown
    readonly overlayUpdates: unknown[] = []
    readonly backgroundUpdates: string[] = []

    constructor(options: unknown) {
      this.options = options
    }

    once(event: string, listener: () => void): void {
      this.listeners.set(event, listener)
    }

    emit(event: string): void {
      this.listeners.get(event)?.()
    }

    isDestroyed(): boolean {
      return this.destroyed
    }

    setTitleBarOverlay(options: unknown): void {
      this.overlayUpdates.push(options)
    }

    setBackgroundColor(color: string): void {
      this.backgroundUpdates.push(color)
    }

    isMinimized(): boolean {
      return this.minimized
    }

    isVisible(): boolean {
      return this.visible
    }

    restore(): void {
      this.minimized = false
      this.actions.push('restore')
    }

    show(): void {
      this.visible = true
      this.actions.push('show')
    }

    focus(): void {
      this.actions.push('focus')
    }

    loadFile(_path: string): Promise<void> {
      return Promise.resolve()
    }

    setMinimized(value: boolean): void {
      this.minimized = value
    }
  }

  const themeListeners = new Set<() => void>()
  const fakeNativeTheme = {
    themeSource: 'system' as 'system' | 'dark' | 'light',
    shouldUseDarkColors: true,
    on(event: string, listener: () => void): void {
      if (event === 'updated') themeListeners.add(listener)
    },
    off(event: string, listener: () => void): void {
      if (event === 'updated') themeListeners.delete(listener)
    },
    emitUpdated(): void {
      for (const listener of themeListeners) listener()
    }
  }

  return { FakeBrowserWindow, fakeNativeTheme }
})

vi.mock('electron', () => ({
  BrowserWindow: fakeElectron.FakeBrowserWindow,
  nativeTheme: fakeElectron.fakeNativeTheme
}))
vi.mock('../src/main/security', () => ({
  hardenWebContents: (): void => undefined,
  isProduction: (): boolean => false
}))
vi.mock('../src/main/paths', () => ({ resourcePath: (name: string): string => name }))
vi.mock('../src/main/ui-bundle', () => ({ resolveUiEntry: (): string => 'index.html' }))

const { applyWindowTheme, createMainWindow } = await import('../src/main/window')

describe('основное окно', () => {
  it('при готовности renderer восстанавливается, показывается и получает фокус', () => {
    const window = createMainWindow({
      rendererOrigin: 'file://',
      log: () => undefined,
      onRendererFailure: () => undefined
    }) as unknown as InstanceType<typeof fakeElectron.FakeBrowserWindow>

    expect(window.options).toMatchObject({
      width: 1_360,
      height: 860,
      minWidth: 960,
      minHeight: 640,
      useContentSize: true,
      center: true,
      titleBarStyle: 'hidden',
      titleBarOverlay: {
        color: '#00000000',
        symbolColor: '#a6b1c2',
        height: 56
      },
      backgroundColor: '#0c1017'
    })
    window.setMinimized(true)
    window.emit('ready-to-show')

    expect(window.actions).toEqual(['restore', 'show', 'focus'])

    applyWindowTheme('light')
    expect(fakeElectron.fakeNativeTheme.themeSource).toBe('light')
    fakeElectron.fakeNativeTheme.shouldUseDarkColors = false
    fakeElectron.fakeNativeTheme.emitUpdated()

    expect(window.backgroundUpdates).toEqual(['#f3f5f9'])
    expect(window.overlayUpdates).toEqual([{
      color: '#00000000',
      symbolColor: '#516075',
      height: 56
    }])
  })
})
