import { app, Menu, Tray, type BrowserWindow } from 'electron'

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
 */

export interface TrayController {
  readonly tray: Tray
  isKeepAlive(): boolean
  forceQuit(): void
  destroy(): void
}

export interface TrayOptions {
  readonly window: BrowserWindow
  readonly log: ShellLog
}

export function createTray(options: TrayOptions): TrayController {
  const tray = new Tray(resourcePath('evohime-agent.ico'))
  let keepAlive = true
  let quitting = false

  const render = (): void => {
    tray.setContextMenu(
      Menu.buildFromTemplate([
        { label: 'Показать EvoHime', click: () => focusWindow(options.window) },
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
  }

  tray.setToolTip('EvoHime')
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
    destroy: () => tray.destroy()
  }
}
