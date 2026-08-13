import { app, BrowserWindow, Notification } from 'electron'

import type { ShellState } from '@shared/api'

import { JsonlLogger } from './diagnostics/logger'
import { readLaunchContext } from './ipc/launch-context'
import { CorePipeClient } from './ipc/pipe-client'
import { dataDirectory, logDirectory } from './paths'
import { ReloadLimiter } from './recovery'
import { hardenProcess, hardenSession, isProduction, type HardeningOptions } from './security'
import { broadcast, registerShellBridge } from './shell-bridge'
import { createTray, type TrayController } from './tray'
import { createMainWindow, focusWindow, loadRenderer } from './window'
import { WorkspaceService, windowChooser } from './workspace-service'
import { WorkspaceStore } from './workspace-store'

/**
 * Electron main process.
 *
 * It owns the window, the tray and the transport to Core — nothing else. The
 * supervisor owns Core lifecycle, the mutex and the Job Object; Core owns every
 * security decision (plan 0, target architecture).
 */

const logger = new JsonlLogger({ directory: logDirectory(), stream: 'main' })
const log: HardeningOptions['log'] = (level, event, fields) => logger.write(level, event, fields)

const launch = readLaunchContext()
const reloadLimiter = new ReloadLimiter()

let mainWindow: BrowserWindow | null = null
let tray: TrayController | null = null
let client: CorePipeClient | null = null
let recoveryMode = false

const rendererOrigin = isProduction()
  ? 'file://'
  : (process.env['ELECTRON_RENDERER_URL'] ?? 'file://')

const hardening: HardeningOptions = { rendererOrigin, log }

if (!app.requestSingleInstanceLock()) {
  // The supervisor owns single-instance for Core; this lock only prevents a
  // second shell. The first instance focuses its window instead.
  log('info', 'shell.second_instance_exit', {})
  app.exit(0)
} else {
  hardenProcess(hardening)

  app.on('second-instance', () => {
    if (mainWindow) {
      focusWindow(mainWindow)
    }
  })

  app.whenReady().then(() => {
    hardenSession(hardening)

    client = new CorePipeClient({ launch, refreshLaunch: () => readLaunchContext(), log })

    client.on('state', (state: ShellState) => broadcast({ kind: 'state', state }))
    client.on('core-event', (event) => {
      broadcast({ kind: 'core-event', event })
      notifyWhenHidden(event.eventType)
    })

    mainWindow = createMainWindow({ ...hardening, onRendererFailure: handleRendererFailure })
    tray = createTray({ window: mainWindow, log })

    // The picker dialog is owned by the main process and opens modal to the
    // shell window; the renderer only ever receives the chosen path.
    registerShellBridge({
      client,
      workspaces: new WorkspaceService({
        store: new WorkspaceStore(WorkspaceStore.defaultPath(dataDirectory())),
        chooseDirectory: windowChooser(mainWindow)
      }),
      log
    })

    log('info', 'shell.started', {
      developerLaunch: launch.developerLaunch,
      packaged: app.isPackaged
    })
    client.start()
  })

  app.on('window-all-closed', () => {
    if (!tray?.isKeepAlive()) {
      app.quit()
    }
  })

  app.on('before-quit', () => {
    client?.stop()
    tray?.destroy()
    log('info', 'shell.stopping', {})
  })
}

function notifyWhenHidden(eventType: string): void {
  if (!mainWindow || mainWindow.isDestroyed() || mainWindow.isVisible() || !Notification.isSupported()) {
    return
  }
  const message = eventType === 'task.completed'
    ? 'Задача завершена'
    : eventType === 'task.failed'
      ? 'Задача завершилась с ошибкой'
      : eventType === 'approval.required'
        ? 'Задаче требуется разрешение'
        : null
  if (message) new Notification({ title: 'EvoHime', body: message }).show()
}

function handleRendererFailure(reason: string): void {
  log('error', 'shell.renderer_failure', { reason })
  if (!mainWindow || mainWindow.isDestroyed() || recoveryMode) {
    return
  }
  if (reloadLimiter.record() === 'reload') {
    void loadRenderer(mainWindow)
    return
  }
  // Past the reload budget the shell stops looping and keeps a minimal
  // recovery surface so diagnostics can still be exported.
  recoveryMode = true
  log('error', 'shell.recovery_mode', { failures: reloadLimiter.recentFailures })
}
