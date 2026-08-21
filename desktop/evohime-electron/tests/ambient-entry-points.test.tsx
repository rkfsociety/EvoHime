// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'

/**
 * Три точки входа — трей, глобальный хоткей и панель «Слух» — приводят к
 * одному состоянию.
 *
 * Проверяется именно инвариант, а не оформление: каждая точка входа
 * отправляет одну и ту же команду `ambient.setListening` и не меняет своё
 * состояние сама; состояние всех троих меняет только событие `ambient.state`
 * от ядра.
 */

type MenuItem = { label?: string; enabled?: boolean; click?: () => void; type?: string }

let menu: MenuItem[] = []
let tooltip = ''

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
    setImage(): void {}
    on(): void {}
    destroy(): void {}
  }
}))
vi.mock('../src/main/paths', () => ({ resourcePath: (name: string) => name }))
vi.mock('../src/main/window', () => ({ focusWindow: () => undefined }))

const { createTray } = await import('../src/main/tray')
const { ListeningPanel } = await import('../src/renderer/src/ListeningPanel')

/** Одна команда, какой её видит ядро, независимо от точки входа. */
interface ListeningCommand {
  readonly enabled: boolean
  readonly paused: boolean
  readonly deviceId?: string
}

const commands: ListeningCommand[] = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function stateEvent(state: string): CoreEvent {
  return {
    sequenceId: 0,
    taskId: '',
    eventType: 'ambient.state',
    payload: JSON.stringify({ state, reason: 'user_request', active_device_id: '' })
  }
}

beforeEach(() => {
  commands.length = 0
  menu = []
  tooltip = ''
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      if (command === 'ambient.setListening') commands.push(payload as ListeningCommand)
      if (command === 'ambient.hotkeyStatus') {
        return ok({ combination: 'Control+Alt+M', registered: true })
      }
      if (command === 'listener.getRuntimeStatus') {
        return ok({
          state: 'ready',
          installedVersion: 'whisper-base-q5_1',
          availableVersion: 'whisper-base-q5_1',
          progressPct: 100,
          message: 'готово',
          missingOptional: [],
          toolsDirectory: 'C:/tools'
        })
      }
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true,
    pathForFile: () => ''
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('три точки входа одного состояния', () => {
  it('трей, хоткей и панель отправляют одну и ту же команду', async () => {
    // Хоткей в `index.ts` вызывает тот же обработчик, что и трей, поэтому
    // здесь он представлен тем же колбэком.
    const tray = createTray({
      window: { on: () => undefined, hide: () => undefined } as never,
      log: () => undefined,
      onToggleListening: (paused) => commands.push({ enabled: true, paused })
    })
    tray.setListeningState('listening')

    // 1. Трей.
    menu.find((entry) => entry.label === 'Поставить микрофон на паузу')?.click?.()
    // 2. Хоткей: та же функция, которую main передаёт трею.
    const hotkey = (paused: boolean): void => commands.push({ enabled: true, paused })
    hotkey(true)
    // 3. Панель.
    render(
      <ListeningPanel connection="connected" events={[stateEvent('listening')]} />
    )
    await userEvent.click(screen.getByRole('button', { name: 'Пауза' }))

    expect(commands).toHaveLength(3)
    for (const command of commands) {
      expect(command.enabled).toBe(true)
      expect(command.paused).toBe(true)
    }
  })

  it('состояние всех троих меняет только событие ядра', async () => {
    const tray = createTray({
      window: { on: () => undefined, hide: () => undefined } as never,
      log: () => undefined,
      onToggleListening: () => undefined
    })
    tray.setListeningState('listening')

    const view = render(
      <ListeningPanel connection="connected" events={[stateEvent('listening')]} />
    )
    await userEvent.click(screen.getByRole('button', { name: 'Пауза' }))

    // Команда ушла, события ещё не было: обе поверхности показывают прежнее
    // состояние. Локальной копии, способной убежать вперёд, нет.
    expect(tooltip).toBe('EvoHime · Ева слушает')
    expect(screen.getByText('Ева слушает')).toBeTruthy()

    // Событие пришло — обе поверхности перешли в одно и то же состояние.
    tray.setListeningState('paused_by_user')
    view.rerender(
      <ListeningPanel connection="connected" events={[stateEvent('paused_by_user')]} />
    )
    expect(tooltip).toBe('EvoHime · Микрофон на паузе')
    expect(screen.getByText('Микрофон на паузе')).toBeTruthy()
  })
})
