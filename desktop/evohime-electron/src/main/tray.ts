import { app, Menu, nativeImage, Tray, type BrowserWindow } from 'electron'
import { existsSync } from 'node:fs'

import type { AppLocale, ListeningState } from '@shared/api'

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
  /** Applies the user-selected locale to native tray labels. */
  setLocale(locale: AppLocale): void
  destroy(): void
}

export interface TrayOptions {
  readonly window: BrowserWindow
  readonly log: ShellLog
  readonly initialLocale?: AppLocale
  /**
   * Просит Core сменить состояние слушания. Трей не меняет своё состояние
   * сам: он ждёт `ambient.state`, иначе трей, панель и хоткей разошлись бы.
   */
  readonly onToggleListening: (paused: boolean) => void
}

/**
 * Подписи состояний слушания для трея.
 *
 * All native tray text follows the same locale as the renderer appearance.
 */
const TRAY_LABELS: Record<AppLocale, Record<ListeningState, string>> = {
  ru: {
    stopped: 'Слушание выключено',
    starting: 'Слушание запускается…',
    listening: 'Ева слушает',
    paused_by_user: 'Микрофон на паузе',
    paused_by_policy: 'Микрофон на паузе по политике',
    device_conflict: 'Микрофон занят другим приложением',
    device_disconnected: 'Микрофон отключён',
    engine_unavailable: 'Слушание: проверка состояния…',
    denied: 'Слушание запрещено'
  },
  en: {
    stopped: 'Listening is off',
    starting: 'Starting listening…',
    listening: 'Eva is listening',
    paused_by_user: 'Microphone is paused',
    paused_by_policy: 'Microphone paused by policy',
    device_conflict: 'Microphone is in use by another app',
    device_disconnected: 'Microphone disconnected',
    engine_unavailable: 'Listening: checking status…',
    denied: 'Listening is disabled'
  }
}

/**
 * Заголовок трея. `null` означает «состояние ещё неизвестно» — и это
 * говорится прямо, а не подменяется словом «выключено».
 */
export function trayTooltip(state: ListeningState | null, locale: AppLocale = 'ru'): string {
  if (state === null) return locale === 'en' ? 'EvoHime · Listening: checking status…' : 'EvoHime · Слушание: проверка состояния…'
  return `EvoHime · ${TRAY_LABELS[locale][state]}`
}

/** Пункт меню паузы: подпись и то, во что перейдёт слушание по нажатию. */
export function trayPauseItem(state: ListeningState | null, locale: AppLocale = 'ru'): {
  readonly label: string
  readonly paused: boolean
  readonly enabled: boolean
} {
  if (state === 'listening' || state === 'starting') {
    return { label: locale === 'en' ? 'Pause microphone' : 'Поставить микрофон на паузу', paused: true, enabled: true }
  }
  if (state === 'paused_by_user') {
    return { label: locale === 'en' ? 'Resume listening' : 'Продолжить слушание', paused: false, enabled: true }
  }
  // Во всех остальных состояниях микрофон и так закрыт: предлагать паузу
  // значило бы обещать действие, которое ничего не изменит.
  return { label: locale === 'en' ? 'Pause microphone' : 'Поставить микрофон на паузу', paused: true, enabled: false }
}

/** Брендовая иконка трея не меняется вместе со статусом слушания. */
export function trayIconName(_state: ListeningState | null): string {
  return 'evohime-agent.ico'
}

export function createTray(options: TrayOptions): TrayController {
  const tray = new Tray(resourcePath('evohime-agent.ico'))
  let keepAlive = true
  let quitting = false
  let listening: ListeningState | null = null
  let locale = options.initialLocale ?? 'ru'

  const render = (): void => {
    const pause = trayPauseItem(listening, locale)
    tray.setContextMenu(
      Menu.buildFromTemplate([
        { label: locale === 'en' ? 'Show EvoHime' : 'Показать EvoHime', click: () => focusWindow(options.window) },
        { type: 'separator' },
        { label: trayTooltip(listening, locale).replace('EvoHime · ', ''), enabled: false },
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
          label: locale === 'en' ? 'Keep session running in background' : 'Держать сессию в фоне',
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
          label: locale === 'en' ? 'Quit' : 'Завершить',
          click: () => {
            quitting = true
            options.log('info', 'shell.force_quit', {})
            app.quit()
          }
        }
      ])
    )
    tray.setToolTip(trayTooltip(listening, locale))
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
    setLocale: (nextLocale) => {
      if (nextLocale === locale) return
      locale = nextLocale
      render()
    },
    destroy: () => tray.destroy()
  }
}
