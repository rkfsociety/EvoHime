import { app, BrowserWindow, ipcMain, Menu, nativeImage, screen, Tray } from 'electron'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { spawn } from 'node:child_process'

import type { ShellLog } from './diagnostics/logger'
import { dataDirectory, resourcePath } from './paths'
import { hardenSession, hardenWebContents, isProduction, type HardeningOptions } from './security'
import { loadUpdateConfig } from './update/config'
import { ModuleUpdateService, shouldApplyBootstrap } from './update/module-update-service'
import { updaterUiStatus, type UpdaterUiStatus } from '@shared/updater'
import { clearUpdaterStart, recordUpdaterStart } from './update/crash-loop'

const STATUS_CHANNEL = 'updater.status'
const CHECK_INTERVAL_MS = 350

export interface UpdaterWindowOptions extends HardeningOptions {
  readonly log: ShellLog
  readonly flushLogs: () => Promise<boolean>
}

/** Runs the standalone Electron updater application. */
export async function runUpdaterApplication(options: UpdaterWindowOptions): Promise<void> {
  await app.whenReady()
  hardenSession(options)
  const installDirectory = installDirectoryFromArgs()
  const config = loadUpdateConfig({
    dataDirectory: dataDirectory(),
    executablePath: join(installDirectory, 'EvoHime.exe'),
    packaged: true
  })
  const crashGuard = recordUpdaterStart(join(dataDirectory(), 'update-state'))
  let shuttingDown = false
  let allowQuitAfterFlush = false
  let shutdownStarted = false
  let updaterTray: Tray | null = null
  const exitAfterLoggerFlush = (): void => {
    shuttingDown = true
    updaterTray?.destroy()
    updaterTray = null
    void options.flushLogs().then((flushed) => {
      if (!flushed) process.stderr.write('EvoHime updater diagnostics flush timed out; exiting with a bounded log tail.\n')
      app.exit(0)
    })
  }
  let updaterWindow: BrowserWindow | null = null
  let gateRunning = false

  const focusUpdaterWindow = (): void => {
    if (!updaterWindow || updaterWindow.isDestroyed()) return
    if (updaterWindow.isMinimized()) updaterWindow.restore()
    if (!updaterWindow.isVisible()) updaterWindow.show()
    updaterWindow.focus()
  }

  const ensureUpdaterTray = (): void => {
    if (updaterTray) return
    const iconFile = resourcePath('evohime-agent.ico')
    const icon = existsSync(iconFile) ? nativeImage.createFromPath(iconFile) : nativeImage.createEmpty()
    const tray = new Tray(icon)
    tray.setToolTip('EvoHime — Центр обновлений')
    tray.setContextMenu(Menu.buildFromTemplate([
      { label: 'Открыть окно', click: () => focusUpdaterWindow() },
      { type: 'separator' },
      { label: 'Закрыть', click: () => { shuttingDown = true; tray.destroy(); updaterTray = null; app.quit() } }
    ]))
    tray.on('click', () => focusUpdaterWindow())
    updaterTray = tray
  }

  app.on('second-instance', () => {
    focusUpdaterWindow()
  })

  const service = new ModuleUpdateService({
    dataDirectory: dataDirectory(),
    branch: config.branch,
    enabled: true,
    updaterPath: join(installDirectory, 'evohime-updater.exe'),
    installDirectory,
    intervalMs: CHECK_INTERVAL_MS,
    emit: () => publish(),
    quitForApply: () => {
      options.log('info', 'updater.quit_for_apply', {})
      exitAfterLoggerFlush()
    },
    onApplyComplete: (succeeded) => {
      if (shuttingDown || !succeeded) {
        options.log('warn', 'updater.apply_finished_without_recheck', { succeeded, shuttingDown })
        return
      }
      options.log('info', 'updater.apply_completed_recheck', {})
      void runGate()
    }
  })

  const publish = (): void => {
    if (updaterWindow && !updaterWindow.isDestroyed()) {
      updaterWindow.webContents.send(STATUS_CHANNEL, updaterUiStatus(service.status))
    }
  }

  const publishFailure = (message: string): void => {
    const current = updaterUiStatus(service.status)
    publishToWindow({
      ...current,
      phase: 'failed',
      heading: 'Обновление не завершено',
      badge: 'Ошибка',
      message,
      canApply: false
    })
  }

  const publishToWindow = (status: UpdaterUiStatus): void => {
    if (updaterWindow && !updaterWindow.isDestroyed()) {
      updaterWindow.webContents.send(STATUS_CHANNEL, status)
    }
  }

  const launchShell = (): void => {
    if (shuttingDown) return
    const shell = join(installDirectory, 'EvoHime.exe')
    const shellExists = existsSync(shell)
    if (!shellExists) {
      const availableModules = service.status.availableModules ?? []
      if (shouldApplyBootstrap(shellExists, availableModules)) {
        void service.prepareComponents(availableModules)
        return
      }
      options.log('error', 'updater.launch_shell_missing', { shell })
      publishFailure(`Файл EvoHime.exe не найден в ${installDirectory}.`)
      return
    }
    try {
      options.log('info', 'updater.launch_shell', { shell })
      const child = spawn(shell, ['--evohime-updater-checked'], { cwd: installDirectory, detached: true, stdio: 'ignore', windowsHide: true })
      child.unref()
      exitAfterLoggerFlush()
    } catch (error) {
      options.log('error', 'updater.launch_shell_failed', { error: error instanceof Error ? error.message : String(error) })
      publishFailure('Не удалось запустить EvoHime.')
    }
  }

  const runGate = async (): Promise<void> => {
    if (shuttingDown || gateRunning) return
    gateRunning = true
    try {
      const gate = await service.runLaunchGate()
      const availableCount = (service.status.availableModules ?? []).length
      options.log('info', 'updater.gate_result', {
        gate,
        phase: service.status.phase,
        availableCount,
        error: service.status.error
      })
      if (gate === 'continue' && service.status.phase !== 'failed' && availableCount === 0) {
        launchShell()
      }
    } finally {
      gateRunning = false
      publish()
    }
  }

  ipcMain.removeHandler('updater.get-status')
  ipcMain.removeHandler('updater.close')
  ipcMain.removeHandler('updater.minimize')
  ipcMain.removeHandler('updater.hide-to-tray')
  ipcMain.removeHandler('updater.retry')
  ipcMain.removeHandler('updater.resize')
  ipcMain.handle('updater.get-status', () => updaterUiStatus(service.status))
  ipcMain.handle('updater.close', () => {
    shuttingDown = true
    updaterTray?.destroy()
    updaterTray = null
    app.quit()
  })
  ipcMain.handle('updater.minimize', () => updaterWindow?.minimize())
  ipcMain.handle('updater.hide-to-tray', () => {
    ensureUpdaterTray()
    updaterWindow?.hide()
  })
  ipcMain.handle('updater.retry', () => {
    if (!shuttingDown && !gateRunning && !crashGuard.blocked) {
      options.log('info', 'updater.retry_requested', {})
      void runGate()
    }
  })
  ipcMain.handle('updater.resize', (_event, rawSize: unknown) => {
    if (!updaterWindow || updaterWindow.isDestroyed()) return
    if (!rawSize || typeof rawSize !== 'object') return
    const candidate = rawSize as { width?: unknown; height?: unknown }
    if (typeof candidate.width !== 'number' || typeof candidate.height !== 'number') return
    if (!Number.isFinite(candidate.width) || !Number.isFinite(candidate.height)) return

    const bounds = updaterWindow.getBounds()
    const workArea = screen.getDisplayMatching(bounds).workArea
    const maxWidth = Math.max(820, Math.min(1040, workArea.width - 32))
    const maxHeight = Math.max(420, Math.min(760, workArea.height - 32))
    const width = Math.max(820, Math.min(maxWidth, Math.round(candidate.width)))
    const height = Math.max(420, Math.min(maxHeight, Math.round(candidate.height)))
    if (bounds.width === width && bounds.height === height) return

    const nextX = Math.max(
      workArea.x,
      Math.min(workArea.x + workArea.width - width, Math.round(bounds.x + (bounds.width - width) / 2))
    )
    const nextY = Math.max(
      workArea.y,
      Math.min(workArea.y + workArea.height - height, Math.round(bounds.y + (bounds.height - height) / 2))
    )
    updaterWindow.setBounds({ x: nextX, y: nextY, width, height }, true)
  })

  updaterWindow = new BrowserWindow({
    width: 860,
    height: 440,
    show: false,
    frame: false,
    resizable: false,
    maximizable: false,
    fullscreenable: false,
    roundedCorners: true,
    backgroundColor: '#090b12',
    icon: join(app.getPath('exe').replace(/[\\/][^\\/]+$/, ''), 'resources', 'evohime-agent.ico'),
    webPreferences: {
      // The updater target has its own output directory (`updater-preload`),
      // separate from the regular shell preload bundle.
      preload: join(__dirname, '../updater-preload/updater.js'),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      webviewTag: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      spellcheck: false,
      devTools: !isProduction()
    }
  })
  hardenWebContents(updaterWindow.webContents, options)
  updaterWindow.webContents.on('did-fail-load', (_event, errorCode, errorDescription, validatedURL, isMainFrame) => {
    options.log('error', 'updater.renderer_load_failed', {
      errorCode,
      errorDescription,
      validatedURL,
      isMainFrame
    })
  })
  updaterWindow.webContents.on('preload-error', (_event, preloadPath, error) => {
    options.log('error', 'updater.preload_error', { preloadPath, error })
  })
  updaterWindow.webContents.on('console-message', (details) => {
    const severity = details.level === 'error' ? 'error' : details.level === 'warning' ? 'warn' : 'info'
    options.log(severity, 'updater.renderer_console', {
      level: details.level,
      message: details.message,
      line: details.lineNumber,
      sourceId: details.sourceId
    })
  })
  updaterWindow.webContents.on('render-process-gone', (_event, details) => {
    options.log('error', 'updater.renderer_gone', { reason: details.reason, exitCode: details.exitCode })
  })
  updaterWindow.webContents.on('did-finish-load', () => {
    options.log('info', 'updater.renderer_loaded', { url: updaterWindow?.webContents.getURL() ?? '' })
  })
  updaterWindow.once('ready-to-show', () => {
    if (!crashGuard.blocked) clearUpdaterStart(join(dataDirectory(), 'update-state'))
    updaterWindow?.show()
  })
  updaterWindow.on('closed', () => {
    updaterWindow = null
    if (!shuttingDown) app.quit()
  })

  const devServerUrl = process.env['ELECTRON_RENDERER_URL']
  if (!isProduction() && devServerUrl) {
    await updaterWindow.loadURL(new URL('updater.html', devServerUrl).toString())
  } else {
    await updaterWindow.loadFile(join(__dirname, '../updater-renderer/updater.html'))
  }

  if (crashGuard.blocked) {
    publishFailure('Updater UI несколько раз подряд завершился при запуске. Требуется восстановление.')
  } else {
    await runGate()
  }
  publish()

  app.on('before-quit', (event) => {
    if (allowQuitAfterFlush) return
    event.preventDefault()
    if (shutdownStarted) return
    shutdownStarted = true
    shuttingDown = true
    service.stop()
    ipcMain.removeHandler('updater.get-status')
    ipcMain.removeHandler('updater.close')
    ipcMain.removeHandler('updater.minimize')
    ipcMain.removeHandler('updater.hide-to-tray')
    ipcMain.removeHandler('updater.retry')
    ipcMain.removeHandler('updater.resize')
    void options.flushLogs().then((flushed) => {
      if (!flushed) process.stderr.write('EvoHime updater diagnostics flush timed out; exiting with a bounded log tail.\n')
      allowQuitAfterFlush = true
      app.quit()
    })
  })
  app.on('window-all-closed', () => app.quit())
}

function installDirectoryFromArgs(): string {
  const value = process.argv
    .slice(2)
    .flatMap((argument, index, argumentsList) => argument === '--install-dir' ? [argumentsList[index + 1] ?? ''] : [])
    .find((argument) => argument.trim().length > 0)
  return resolve(value ?? app.getPath('exe').replace(/[\\/][^\\/]+$/, ''))
}
