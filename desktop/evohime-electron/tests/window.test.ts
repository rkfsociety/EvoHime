import { describe, expect, it, vi } from 'vitest'

const fakeElectron = vi.hoisted(() => {
  class FakeBrowserWindow {
    readonly webContents = { on: (): void => undefined }
    readonly actions: string[] = []
    private readonly listeners = new Map<string, () => void>()
    private minimized = false
    private visible = false

    constructor(_options: unknown) {}

    once(event: string, listener: () => void): void {
      this.listeners.set(event, listener)
    }

    emit(event: string): void {
      this.listeners.get(event)?.()
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

  return { FakeBrowserWindow }
})

vi.mock('electron', () => ({ BrowserWindow: fakeElectron.FakeBrowserWindow }))
vi.mock('../src/main/security', () => ({
  hardenWebContents: (): void => undefined,
  isProduction: (): boolean => false
}))
vi.mock('../src/main/paths', () => ({ resourcePath: (name: string): string => name }))
vi.mock('../src/main/ui-bundle', () => ({ resolveUiEntry: (): string => 'index.html' }))

const { createMainWindow } = await import('../src/main/window')

describe('основное окно', () => {
  it('при готовности renderer восстанавливается, показывается и получает фокус', () => {
    const window = createMainWindow({
      rendererOrigin: 'file://',
      log: () => undefined,
      onRendererFailure: () => undefined
    }) as unknown as InstanceType<typeof fakeElectron.FakeBrowserWindow>

    window.setMinimized(true)
    window.emit('ready-to-show')

    expect(window.actions).toEqual(['restore', 'show', 'focus'])
  })
})
