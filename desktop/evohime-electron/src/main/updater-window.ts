import { app, BrowserWindow, ipcMain } from 'electron'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { spawn } from 'node:child_process'

import type { ShellLog } from './diagnostics/logger'
import { dataDirectory } from './paths'
import { hardenSession, hardenWebContents, isProduction, type HardeningOptions } from './security'
import { loadUpdateConfig } from './update/config'
import { ModuleUpdateService } from './update/module-update-service'
import { updaterUiStatus, type UpdaterUiStatus } from '@shared/updater'

const STATUS_CHANNEL = 'updater.status'
const CHECK_INTERVAL_MS = 350

export interface UpdaterWindowOptions extends HardeningOptions {
  readonly log: ShellLog
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
  const service = new ModuleUpdateService({
    dataDirectory: dataDirectory(),
    branch: config.branch,
    enabled: true,
    updaterPath: join(installDirectory, 'evohime-updater.exe'),
    installDirectory,
    intervalMs: CHECK_INTERVAL_MS,
    emit: () => publish()
  })

  let updaterWindow: BrowserWindow | null = null
  let completionTimer: NodeJS.Timeout | null = null
  let shuttingDown = false

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
      heading: 'Проверка требует внимания',
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
    if (!existsSync(shell)) {
      publishFailure(`Файл EvoHime.exe не найден в ${installDirectory}.`)
      return
    }
    try {
      const child = spawn(shell, [], { cwd: installDirectory, detached: true, stdio: 'ignore', windowsHide: true })
      child.unref()
      shuttingDown = true
      app.exit(0)
    } catch {
      publishFailure('Не удалось запустить EvoHime.')
    }
  }

  const waitForApply = (): void => {
    if (completionTimer !== null) return
    completionTimer = setInterval(() => {
      publish()
      const status = service.status
      if (status.phase === 'failed') {
        clearInterval(completionTimer!)
        completionTimer = null
        return
      }
      if (status.phase !== 'up-to-date') return
      clearInterval(completionTimer!)
      completionTimer = null
      const pendingReplacement = existsSync(join(dataDirectory(), 'update-state', 'updater-relaunch.pending'))
      if (pendingReplacement) {
        shuttingDown = true
        app.exit(0)
      } else {
        launchShell()
      }
    }, CHECK_INTERVAL_MS)
    completionTimer.unref?.()
  }

  ipcMain.removeHandler('updater.get-status')
  ipcMain.removeHandler('updater.apply')
  ipcMain.removeHandler('updater.launch')
  ipcMain.removeHandler('updater.close')
  ipcMain.removeHandler('updater.minimize')
  ipcMain.handle('updater.get-status', () => updaterUiStatus(service.status))
  ipcMain.handle('updater.apply', async () => {
    if (!service.status.availableModules?.length) return
    await service.prepareComponents(service.status.availableModules)
    waitForApply()
  })
  ipcMain.handle('updater.launch', () => launchShell())
  ipcMain.handle('updater.close', () => {
    shuttingDown = true
    app.quit()
  })
  ipcMain.handle('updater.minimize', () => updaterWindow?.minimize())

  updaterWindow = new BrowserWindow({
    width: 1_020,
    height: 680,
    minWidth: 760,
    minHeight: 560,
    show: false,
    frame: false,
    resizable: true,
    roundedCorners: true,
    backgroundColor: '#090b12',
    icon: join(installDirectory, 'resources', 'evohime-agent.ico'),
    webPreferences: {
      preload: join(__dirname, '../preload/updater.js'),
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
  updaterWindow.once('ready-to-show', () => updaterWindow?.show())
  updaterWindow.on('closed', () => {
    updaterWindow = null
    if (!shuttingDown) app.quit()
  })

  const devServerUrl = process.env['ELECTRON_RENDERER_URL']
  if (!isProduction() && devServerUrl) {
    await updaterWindow.loadURL(new URL('updater.html', devServerUrl).toString())
  } else {
    await updaterWindow.loadFile(join(__dirname, '../ui-bundle/updater.html'))
  }

  service.runLaunchGate()
  await service.check()
  publish()

  app.on('before-quit', () => {
    shuttingDown = true
    service.stop()
    if (completionTimer !== null) clearInterval(completionTimer)
    ipcMain.removeHandler('updater.get-status')
    ipcMain.removeHandler('updater.apply')
    ipcMain.removeHandler('updater.launch')
    ipcMain.removeHandler('updater.close')
    ipcMain.removeHandler('updater.minimize')
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
