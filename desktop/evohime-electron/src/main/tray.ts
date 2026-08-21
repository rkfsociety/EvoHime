import { app, Menu, nativeImage, Tray, type BrowserWindow } from 'electron'
import { existsSync } from 'node:fs'

import type { ListeningState } from '@shared/api'

import type { ShellLog } from './diagnostics/logger'
import { resourcePath } from './paths'
import { focusWindow } from './window'

/**
 * Tray surface and quit policy (plan 0, stage 4).
 *
 * Closing the window while keep-alive is on hides it and keeps the session
 * alive; Force Quit restores the ordinary quit policy and releases the
 * supervisor liveness handle. The tray never owns Core lifecycle: the
 * supervisor does.
 *
 * С этапа 04.5 трей ещё и показывает, слушают ли пользователя, и даёт паузу.
 * Собственной копии состояния у него нет: он рисует то, что прислал Core, и
 * по нажатию отправляет ту же команду, что панель и хоткей.
 */

export interface TrayController {
  readonly tray: Tray
  isKeepAlive(): boolean
  forceQuit(): void
  /** Перерисовывает индикатор по состоянию, пришедшему от Core. */
  setListeningState(state: ListeningState | null): void
  destroy(): void
}

export interface TrayOptions {
  readonly window: BrowserWindow
  readonly log: ShellLog
  /**
   * Просит Core сменить состояние слушания. Трей не меняет своё состояние
   * сам: он ждёт `ambient.state`, иначе трей, панель и хоткей разошлись бы.
   */
  readonly onToggleListening: (paused: boolean) => void
}

/**
 * Подписи состояний слушания для трея.
 *
 * Системы локализации в проекте нет: строки — русские литералы в
 * модуль-локальной константной карте, как `STATE_LABELS` в `App.tsx`.
 */
const TRAY_LABELS: Record<ListeningState, string> = {
  stopped: 'Слушание выключено',
  starting: 'Слушание запускается…',
  listening: 'Ева слушает',
  paused_by_user: 'Микрофон на паузе',
  paused_by_policy: 'Микрофон на паузе по политике',
  device_conflict: 'Микрофон занят другим приложением',
  device_disconnected: 'Микрофон отключён',
  engine_unavailable: 'Слушание: проверка состояния…',
  denied: 'Слушание запрещено'
}

/**
 * Заголовок трея. `null` означает «состояние ещё неизвестно» — и это
 * говорится прямо, а не подменяется словом «выключено».
 */
export function trayTooltip(state: ListeningState | null): string {
  if (state === null) return 'EvoHime · Слушание: проверка состояния…'
  return `EvoHime · ${TRAY_LABELS[state]}`
}

/** Пункт меню паузы: подпись и то, во что перейдёт слушание по нажатию. */
export function trayPauseItem(state: ListeningState | null): {
  readonly label: string
  readonly paused: boolean
  readonly enabled: boolean
} {
  if (state === 'listening' || state === 'starting') {
    return { label: 'Поставить микрофон на паузу', paused: true, enabled: true }
  }
  if (state === 'paused_by_user') {
    return { label: 'Продолжить слушание', paused: false, enabled: true }
  }
  // Во всех остальных состояниях микрофон и так закрыт: предлагать паузу
  // значило бы обещать действие, которое ничего не изменит.
  return { label: 'Поставить микрофон на паузу', paused: true, enabled: false }
}

/** Иконка активного слушания отличается от обычной, если она поставлена. */
export function trayIconName(state: ListeningState | null): string {
  return state === 'listening' ? 'evohime-agent-listening.ico' : 'evohime-agent.ico'
}

export function createTray(options: TrayOptions): TrayController {
  const tray = new Tray(resourcePath('evohime-agent.ico'))
  let keepAlive = true
  let quitting = false
  let listening: ListeningState | null = null

  const render = (): void => {
    const pause = trayPauseItem(listening)
    tray.setContextMenu(
      Menu.buildFromTemplate([
        { label: 'Показать EvoHime', click: () => focusWindow(options.window) },
        { type: 'separator' },
        { label: trayTooltip(listening).replace('EvoHime · ', ''), enabled: false },
        {
          label: pause.label,
          enabled: pause.enabled,
          click: () => {
            options.log('info', 'shell.ambient_tray_toggle', { paused: pause.paused })
            options.onToggleListening(pause.paused)
          }
        },
        { type: 'separator' },
        {
          label: 'Держать сессию в фоне',
          type: 'checkbox',
          checked: keepAlive,
          click: () => {
            keepAlive = !keepAlive
            options.log('info', 'shell.keep_alive_changed', { keepAlive })
            render()
          }
        },
        { type: 'separator' },
        {
          label: 'Завершить',
          click: () => {
            quitting = true
            options.log('info', 'shell.force_quit', {})
            app.quit()
          }
        }
      ])
    )
    tray.setToolTip(trayTooltip(listening))
    // Вариант иконки необязателен: если файла нет, остаётся обычная. Падать
    // из-за оформления трея нельзя.
    const iconPath = resourcePath(trayIconName(listening))
    if (existsSync(iconPath)) {
      tray.setImage(nativeImage.createFromPath(iconPath))
    }
  }

  tray.on('double-click', () => focusWindow(options.window))
  render()

  options.window.on('close', (event) => {
    if (keepAlive && !quitting) {
      event.preventDefault()
      options.window.hide()
    }
  })

  return {
    tray,
    isKeepAlive: () => keepAlive,
    forceQuit: () => {
      quitting = true
      app.quit()
    },
    setListeningState: (state) => {
      if (state === listening) return
      listening = state
      render()
    },
    destroy: () => tray.destroy()
  }
}
