import { beforeEach, describe, expect, it, vi } from 'vitest'

/**
 * Индикатор слушания в трее (план 04.5).
 *
 * Трей — одна из трёх точек входа. Своего состояния у него нет: он рисует то,
 * что прислало ядро, и по нажатию отправляет ту же команду, что панель и
 * хоткей. Проверяется именно это, а не оформление меню.
 */

type MenuItem = { label?: string; enabled?: boolean; click?: () => void; type?: string }

const listeners = new Map<string, (event: { preventDefault: () => void }) => void>()
let menu: MenuItem[] = []
let tooltip = ''
let image = ''

vi.mock('electron', () => ({
  app: { quit: () => undefined },
  nativeImage: { createFromPath: (path: string) => path },
  Menu: { buildFromTemplate: (template: MenuItem[]) => template },
  Tray: class {
    setContextMenu(template: MenuItem[]): void {
      menu = template
    }
    setToolTip(value: string): void {
      tooltip = value
    }
    setImage(value: string): void {
      image = value
    }
    on(): void {}
    destroy(): void {}
  }
}))

vi.mock('../src/main/paths', () => ({ resourcePath: (name: string) => name }))
vi.mock('../src/main/window', () => ({ focusWindow: () => undefined }))
vi.mock('node:fs', () => {
  const existsSync = (): boolean => true
  return { existsSync, default: { existsSync } }
})

const { createTray, trayIconName, trayPauseItem, trayTooltip } = await import('../src/main/tray')

function fakeWindow(): unknown {
  return {
    on: (event: string, listener: (event: { preventDefault: () => void }) => void) =>
      listeners.set(event, listener),
    hide: () => undefined
  }
}

function item(label: string): MenuItem | undefined {
  return menu.find((entry) => entry.label === label)
}

describe('индикатор слушания в трее', () => {
  beforeEach(() => {
    listeners.clear()
    menu = []
    tooltip = ''
    image = ''
  })

  it('неизвестное состояние показывается проверкой, а не «выключено»', () => {
    expect(trayTooltip(null)).toBe('EvoHime · Слушание: проверка состояния…')
    expect(trayTooltip('engine_unavailable')).toContain('проверка состояния')
    expect(trayTooltip('stopped')).toBe('EvoHime · Слушание выключено')
  })

  it('иконка активного слушания отличается от обычной', () => {
    expect(trayIconName('listening')).toBe('evohime-agent-listening.ico')
    expect(trayIconName('paused_by_user')).toBe('evohime-agent.ico')
    expect(trayIconName(null)).toBe('evohime-agent.ico')
  })

  it('пункт паузы предлагает действие, которое что-то изменит', () => {
    expect(trayPauseItem('listening')).toEqual({
      label: 'Поставить микрофон на паузу',
      paused: true,
      enabled: true
    })
    expect(trayPauseItem('paused_by_user')).toEqual({
      label: 'Продолжить слушание',
      paused: false,
      enabled: true
    })
    // Микрофон и так закрыт: обещать паузу здесь значило бы обещать пустое
    // действие.
    expect(trayPauseItem('device_disconnected').enabled).toBe(false)
    expect(trayPauseItem(null).enabled).toBe(false)
  })

  it('перерисовывается по состоянию от ядра и отправляет одну команду', () => {
    const toggles: boolean[] = []
    const tray = createTray({
      window: fakeWindow() as never,
      log: () => undefined,
      onToggleListening: (paused) => toggles.push(paused)
    })

    tray.setListeningState('listening')
    expect(tooltip).toBe('EvoHime · Ева слушает')
    expect(image).toBe('evohime-agent-listening.ico')

    item('Поставить микрофон на паузу')?.click?.()
    expect(toggles).toEqual([true])
    // Состояние не поменялось само: трей ждёт `ambient.state` от ядра.
    expect(tooltip).toBe('EvoHime · Ева слушает')

    tray.setListeningState('paused_by_user')
    expect(tooltip).toBe('EvoHime · Микрофон на паузе')
    expect(image).toBe('evohime-agent.ico')
    item('Продолжить слушание')?.click?.()
    expect(toggles).toEqual([true, false])
  })
})
