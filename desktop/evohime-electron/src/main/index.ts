import { app, BrowserWindow, Notification, safeStorage } from 'electron'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { spawn, type ChildProcess } from 'node:child_process'

import type { ShellState } from '@shared/api'

import { ChatStore } from './chat-store'
import { JsonlLogger } from './diagnostics/logger'
import { hasLiveSupervisor, readLaunchContext } from './ipc/launch-context'
import { CorePipeClient } from './ipc/pipe-client'
import { dataDirectory, logDirectory } from './paths'
import { ProviderStore } from './provider-store'
import { ReloadLimiter } from './recovery'
import { hardenProcess, hardenSession, isProduction, type HardeningOptions } from './security'
import { broadcast, registerShellBridge } from './shell-bridge'
import { createTray, type TrayController } from './tray'
import { BuildLog } from './update/build-log'
import { BUILD_WORKER_FLAG, runBuildWorkerProcess } from './update/build-worker'
import { loadUpdateConfig } from './update/config'
import { UpdateService } from './update/update-service'
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
let updates: UpdateService | null = null

// safeStorage is only usable after the app is ready, so the store is created
// lazily inside whenReady rather than at module scope.
let providers: ProviderStore | null = null

const rendererOrigin = isProduction()
  ? 'file://'
  : (process.env['ELECTRON_RENDERER_URL'] ?? 'file://')

const hardening: HardeningOptions = { rendererOrigin, log }

if (process.argv.includes(BUILD_WORKER_FLAG)) {
  void runBuildWorkerProcess(process.argv.slice(2))
    .then(() => app.exit(0))
    .catch((error: unknown) => {
      console.error(error instanceof Error ? error.message : String(error))
      app.exit(1)
    })
} else if (!app.requestSingleInstanceLock()) {
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

    providers = new ProviderStore(ProviderStore.defaultPath(dataDirectory()), {
      isAvailable: () => safeStorage.isEncryptionAvailable(),
      encrypt: (value) => safeStorage.encryptString(value),
      decrypt: (value) => safeStorage.decryptString(value)
    })

    // The client is created before the supervisor exists: it re-reads the launch
    // context on every connection attempt, so nothing is lost by connecting
    // after the update gate has finished with the installed files.
    client = new CorePipeClient({
      launch: readLaunchContext(),
      refreshLaunch: () => readLaunchContext(),
      log
    })

    client.on('state', (state: ShellState) => broadcast({ kind: 'state', state }))
    client.on('core-event', (event) => {
      broadcast({ kind: 'core-event', event })
      notifyWhenHidden(event.eventType)
    })

    mainWindow = createMainWindow({ ...hardening, onRendererFailure: handleRendererFailure })
    tray = createTray({ window: mainWindow, log })
    updates = createUpdateService()

    // The picker dialog is owned by the main process and opens modal to the
    // shell window; the renderer only ever receives the chosen path.
    registerShellBridge({
      client,
      workspaces: new WorkspaceService({
        store: new WorkspaceStore(WorkspaceStore.defaultPath(dataDirectory())),
        chooseDirectory: windowChooser(mainWindow)
      }),
      providers,
      chats: new ChatStore(ChatStore.defaultPath(dataDirectory())),
      restartCore,
      updates,
      log
    })

    // Nothing may hold the installed binaries open while a staged rebuild is
    // swapped in, so the gate runs before the supervisor is started. An updater
    // that fails in an unforeseen way must never keep the client from starting:
    // the installed build is always launchable.
    let gate: Awaited<ReturnType<UpdateService['runLaunchGate']>> = 'continue'
    try {
      gate = await updates.runLaunchGate()
    } catch (error) {
      log('error', 'shell.update_gate_failed', { error })
    }
    if (gate === 'applying') {
      log('info', 'shell.update_applying', {})
      return
    }

    const launch = await ensureSupervisorSession()
    supervisorLivenessTimer = monitorSupervisorLiveness()

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
    updates?.stop()
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

/**
 * Source updater.
 *
 * A development run is never updated: it already builds from the developer's own
 * checkout, and `app.getPath('exe')` there points at the Electron binary in
 * `node_modules` rather than at an installation.
 */
function createUpdateService(): UpdateService {
  const config = loadUpdateConfig({
    dataDirectory: dataDirectory(),
    executablePath: app.getPath('exe')
  })
  const enabled = config.enabled && app.isPackaged
  return new UpdateService({
    config: { ...config, enabled, launchPolicy: enabled ? config.launchPolicy : 'off' },
    emit: (status) => broadcast({ kind: 'update', status }),
    log,
    // The UI shows one build line; the whole output stays on disk, because a
    // failed rebuild cannot be explained from its last line alone.
    buildLog: new BuildLog(join(logDirectory(), 'update-build.log')),
    quit: quitForUpdate
  })
}

/** Grace period before the shell stops asking to exit and simply exits. */
const UPDATE_QUIT_GRACE_MS = 5_000

/**
 * Ends the process so the staged package can replace the installed files.
 *
 * `app.quit()` on its own is not enough: with keep-alive on, the tray cancels
 * the window close and hides the window instead, and the shell keeps running.
 * The transaction then waits for a process that never exits, finds the
 * installation still locked by it, and gives up — the update stays unapplied
 * while the UI sits in «Применяю обновление…» forever. Quitting goes through the
 * tray's own quit path, which clears that veto, and the timer is the backstop
 * for anything else that could refuse to close.
 */
function quitForUpdate(): void {
  if (tray) {
    tray.forceQuit()
  } else {
    app.quit()
  }
  const timer = setTimeout(() => {
    log('warn', 'shell.update_quit_forced', {})
    app.exit(0)
  }, UPDATE_QUIT_GRACE_MS)
  timer.unref?.()
}

/**
 * Applies newly stored credentials. Core builds its model gateway from the
 * environment at startup, so the supervisor that owns it is restarted and the
 * pipe client reconnects to the fresh session.
 */
async function restartCore(): Promise<boolean> {
  log('info', 'shell.core_restart_requested', {})
  client?.stop()
  stopSupervisor()
  const launch = await ensureSupervisorSession(true)
  client?.start()
  if (launch.developerLaunch) {
    log('warn', 'shell.core_restart_no_supervisor', {})
    return false
  }
  return true
}

/** Kills the supervisor we own; its Job Object takes Core down with it. */
function stopSupervisor(): void {
  const child = supervisorProcess
  supervisorProcess = null
  if (child && !child.killed) {
    child.kill()
  }
  const pid = readLaunchContext().supervisorPid
  if (pid && processIsAlive(pid)) {
    try {
      process.kill(pid)
    } catch (error) {
      log('warn', 'shell.supervisor_kill_failed', { error })
    }
  }
}

async function ensureSupervisorSession(
  force = false
): Promise<ReturnType<typeof readLaunchContext>> {
  const current = readLaunchContext()
  if (!force && hasLiveSupervisor(current, processIsAlive)) {
    return current
  }
  if (!current.developerLaunch) {
    log('warn', 'shell.stale_supervisor_context', { pid: current.supervisorPid ?? 0 })
  }

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
      // Credentials stored through the settings surface win over anything the
      // ambient environment carries, so the UI is the single source of truth.
      ...(providers?.environment() ?? {}),
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

function processIsAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
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
