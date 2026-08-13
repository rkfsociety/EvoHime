import { app, BrowserWindow, Notification } from 'electron'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { spawn, type ChildProcess } from 'node:child_process'

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

const reloadLimiter = new ReloadLimiter()

let mainWindow: BrowserWindow | null = null
let tray: TrayController | null = null
let client: CorePipeClient | null = null
let supervisorProcess: ChildProcess | null = null
let supervisorLivenessTimer: NodeJS.Timeout | null = null
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

  app.whenReady().then(async () => {
    hardenSession(hardening)

    const launch = await ensureSupervisorSession()
    client = new CorePipeClient({ launch, refreshLaunch: () => readLaunchContext(), log })
    supervisorLivenessTimer = monitorSupervisorLiveness()

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
    if (supervisorLivenessTimer) {
      clearInterval(supervisorLivenessTimer)
      supervisorLivenessTimer = null
    }
    if (supervisorProcess && !supervisorProcess.killed) {
      supervisorProcess.kill()
      supervisorProcess = null
    }
    tray?.destroy()
    log('info', 'shell.stopping', {})
  })
}

async function ensureSupervisorSession(): Promise<ReturnType<typeof readLaunchContext>> {
  const current = readLaunchContext()
  if (!current.developerLaunch) return current

  const supervisorPath = supervisorExecutablePath()
  if (!supervisorPath || !existsSync(supervisorPath)) {
    log('warn', 'shell.supervisor_missing', {})
    return current
  }
  const child = spawn(supervisorPath, [], {
    cwd: dirname(supervisorPath),
    detached: false,
    windowsHide: true,
    stdio: 'ignore',
    env: {
      ...process.env,
      EVOHIME_CORE_EXE: coreExecutablePath() ?? process.env['EVOHIME_CORE_EXE'],
      EVOHIME_DATA_DIR: dataDirectory()
    }
  })
  supervisorProcess = child
  child.once('error', (error) => {
    log('error', 'shell.supervisor_process_error', { error })
  })
  child.once('exit', (code, signal) => {
    if (supervisorProcess === child) supervisorProcess = null
    log('warn', 'shell.supervisor_exited', { code: code ?? -1, signal: signal ?? '' })
  })
  child.unref()
  log('info', 'shell.supervisor_started', {})

  const deadline = Date.now() + 5_000
  while (Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 100))
    const next = readLaunchContext()
    if (!next.developerLaunch) return next
  }
  log('warn', 'shell.supervisor_context_timeout', {})
  return readLaunchContext()
}

function supervisorExecutablePath(): string | null {
  return process.env['EVOHIME_SUPERVISOR_EXE'] || packagedSibling('evohime-supervisor.exe')
}

function coreExecutablePath(): string | null {
  return process.env['EVOHIME_CORE_EXE'] || packagedSibling('evohime-core.exe')
}

function packagedSibling(name: string): string | null {
  const candidates = [
    join(process.resourcesPath, '..', name),
    join(process.cwd(), name),
    join(app.getAppPath(), '..', name)
  ]
  return candidates.find((candidate) => existsSync(candidate)) ?? candidates[0] ?? null
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

function monitorSupervisorLiveness(): NodeJS.Timeout {
  const timer = setInterval(() => {
    const launch = readLaunchContext()
    const pid = launch.supervisorPid
    if (launch.developerLaunch || !pid) return
    try {
      process.kill(pid, 0)
    } catch {
      log('error', 'shell.supervisor_owner_lost', { pid })
      client?.stop()
      clearInterval(timer)
    }
  }, 1_000)
  timer.unref()
  return timer
}
