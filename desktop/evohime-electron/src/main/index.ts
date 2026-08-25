import { app, BrowserWindow, globalShortcut, net, Notification, safeStorage } from 'electron'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { spawn, type ChildProcess } from 'node:child_process'

import type { AmbientHotkeyStatus, ListeningState, ShellState } from '@shared/api'

import { ChatStore } from './chat-store'
import { CodexService } from './codex-service'
import { JsonlLogger } from './diagnostics/logger'
import { hasLiveSupervisor, readLaunchContext } from './ipc/launch-context'
import { CorePipeClient } from './ipc/pipe-client'
import { dataDirectory, logDirectory } from './paths'
import { ProviderStore } from './provider-store'
import { ReloadLimiter } from './recovery'
import { hardenProcess, hardenSession, isProduction, type HardeningOptions } from './security'
import { broadcast, registerShellBridge } from './shell-bridge'
import { supervisorEnvironment } from './supervisor-env'
import { createTray, type TrayController } from './tray'
import { BuildLog } from './update/build-log'
import { BUILD_WORKER_FLAG, runBuildWorkerProcess } from './update/build-worker'
import { loadUpdateConfig } from './update/config'
import { resolveGithubToken } from './update/github-token'
import { ListenerRuntimeService } from './update/listener-runtime'
import { UpdateService } from './update/update-service'
import { createOverlay, type OverlayController } from './overlay'
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
let overlay: OverlayController | null = null
let client: CorePipeClient | null = null
let supervisorProcess: ChildProcess | null = null
let supervisorLivenessTimer: NodeJS.Timeout | null = null
let recoveryMode = false
let updates: UpdateService | null = null
let listenerRuntime: ListenerRuntimeService | null = null
let codex: CodexService | null = null

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
    codex = new CodexService(CodexService.defaultPath(dataDirectory()), log, async (model) => {
      providers?.saveCodexModel(model)
      await restartCore()
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
      observeAmbientEvent(event.eventType, event.payload)
      notifyWhenHidden(event.eventType)
      // Индикатор поверх всех окон коротко вспыхивает, когда ядро распознало
      // обращение «Ева, …» — тот же сигнал, что панель показывает карточкой.
      if (event.eventType === 'ambient.voice_command') overlay?.flashHeard()
    })

    mainWindow = createMainWindow({ ...hardening, onRendererFailure: handleRendererFailure })
    tray = createTray({
      window: mainWindow,
      log,
      onToggleListening: requestAmbientListening
    })
    overlay = createOverlay()
    updates = createUpdateService()
    listenerRuntime = createListenerRuntimeService()

    // The picker dialog is owned by the main process and opens modal to the
    // shell window; the renderer only ever receives the chosen path.
    registerShellBridge({
      client,
      workspaces: new WorkspaceService({
        store: new WorkspaceStore(WorkspaceStore.defaultPath(dataDirectory())),
        chooseDirectory: windowChooser(mainWindow)
      }),
      providers,
      codex: codex!,
      chats: new ChatStore(ChatStore.defaultPath(dataDirectory())),
      restartCore,
      updates,
      listenerRuntime: listenerRuntime!,
      ambientHotkey: ambientHotkeyStatus,
      log
    })

    // Третья точка входа поднимается здесь, а не в панели: она глобальная и
    // должна работать при скрытом окне.
    registerAmbientHotkey()

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
    // Состояние слушания спрашивается сразу: панель может быть закрыта, а
    // трей обязан показывать правду с первой секунды.
    requestAmbientStatus()
    // Состояние речевого рантайма выясняется на старте, чтобы «Слух» не
    // предлагал включить микрофон при неустановленном движке.
    void listenerRuntime.check().catch((error: unknown) => {
      log('warn', 'shell.listener_runtime_check_failed', { error })
    })
  })

  app.on('window-all-closed', () => {
    if (!tray?.isKeepAlive()) {
      app.quit()
    }
  })

  app.on('will-quit', () => {
    // Хоткей снимается явно: незанятая комбинация после выхода — часть
    // контракта с другими приложениями.
    globalShortcut.unregisterAll()
    ambientHotkeyRegistered = false
  })

  app.on('before-quit', () => {
    client?.stop()
    codex?.dispose()
    updates?.stop()
    if (ambientStatusTimer) {
      clearTimeout(ambientStatusTimer)
      ambientStatusTimer = null
    }
    if (supervisorLivenessTimer) {
      clearInterval(supervisorLivenessTimer)
      supervisorLivenessTimer = null
    }
    if (supervisorProcess && !supervisorProcess.killed) {
      supervisorProcess.kill()
      supervisorProcess = null
    }
    tray?.destroy()
    overlay?.destroy()
    log('info', 'shell.stopping', {})
  })
}

/**
 * Глобальный хоткей паузы микрофона.
 *
 * Третья точка входа наравне с треем и панелью. Комбинацию может занять
 * другое приложение — тогда `register` возвращает `false`, и оболочка
 * говорит об этом, а не делает вид, что хоткей работает.
 */
const AMBIENT_HOTKEY = 'Control+Alt+M'

/**
 * Сколько ждать ответа на `GetAmbientStatus`, прежде чем считать состояние
 * неизвестным. Молчание дольше этого — не «выключено», а «неизвестно».
 */
const AMBIENT_STATUS_TIMEOUT_MS = 5_000

let ambientState: ListeningState | null = null
let ambientStatusTimer: NodeJS.Timeout | null = null
let ambientHotkeyRegistered = false

/** Доступность хоткея в том виде, в каком её показывает панель «Слух». */
function ambientHotkeyStatus(): AmbientHotkeyStatus {
  return { combination: AMBIENT_HOTKEY, registered: ambientHotkeyRegistered }
}

/**
 * Регистрирует хоткей паузы.
 *
 * Занятая комбинация — штатный исход, а не ошибка запуска: приложение
 * работает дальше, а панель показывает, что третья точка входа недоступна.
 */
function registerAmbientHotkey(): void {
  try {
    ambientHotkeyRegistered = globalShortcut.register(AMBIENT_HOTKEY, () => {
      const paused = ambientState === 'listening' || ambientState === 'starting'
      log('info', 'shell.ambient_hotkey', { paused })
      requestAmbientListening(paused)
    })
  } catch (error) {
    ambientHotkeyRegistered = false
    log('warn', 'shell.ambient_hotkey_failed', { error })
  }
  if (!ambientHotkeyRegistered) {
    log('warn', 'shell.ambient_hotkey_taken', { combination: AMBIENT_HOTKEY })
  }
}

/**
 * Единственный путь к смене состояния слушания из main-процесса.
 *
 * Трей и хоткей не меняют состояние сами: они отправляют команду и ждут
 * `ambient.state`. Локальной копии, способной разойтись с панелью, нет.
 */
function requestAmbientListening(paused: boolean): void {
  client?.send({ setAmbientListening: { enabled: true, paused, deviceId: '' } })
}

/**
 * Спрашивает состояние слушания и заводит таймер недоверия.
 *
 * Ответ приходит событием `ambient.status`; если он не пришёл за отведённое
 * время, состояние становится неизвестным, а индикатор — предупреждающим.
 */
function requestAmbientStatus(): void {
  if (client?.send({ getAmbientStatus: {} }) !== 'queued') {
    setAmbientState(null)
    return
  }
  if (ambientStatusTimer) clearTimeout(ambientStatusTimer)
  ambientStatusTimer = setTimeout(() => {
    ambientStatusTimer = null
    log('warn', 'shell.ambient_status_timeout', {})
    setAmbientState(null)
  }, AMBIENT_STATUS_TIMEOUT_MS)
  ambientStatusTimer.unref?.()
}

function setAmbientState(state: ListeningState | null): void {
  ambientState = state
  tray?.setListeningState(state)
  overlay?.setListeningState(state)
}

/**
 * Разбирает ambient-событие Core.
 *
 * Ответ с ошибкой или неразбираемая полезная нагрузка означают «состояние
 * неизвестно», а не «выключено»: это ровно тот случай, когда индикатор
 * обязан предупреждать.
 */
function observeAmbientEvent(eventType: string, payload: string): void {
  if (eventType !== 'ambient.state' && eventType !== 'ambient.status') return
  if (ambientStatusTimer) {
    clearTimeout(ambientStatusTimer)
    ambientStatusTimer = null
  }
  try {
    const value = JSON.parse(payload) as { state?: unknown }
    setAmbientState(typeof value.state === 'string' ? (value.state as ListeningState) : null)
  } catch {
    setAmbientState(null)
  }
}

/**
 * Source updater.
 *
 * A development run is never updated: it already builds from the developer's own
 * checkout, and `app.getPath('exe')` there points at the Electron binary in
 * `node_modules` rather than at an installation.
 */
/**
 * Набор рантайма распознавания речи.
 *
 * Сервис живёт отдельно от `UpdateService`: обновление продукта и загрузка
 * речевого рантайма — разные решения пользователя и разные релизные ассеты.
 * Общими остаются только репозиторий и способ добыть токен GitHub.
 */
function createListenerRuntimeService(): ListenerRuntimeService {
  const config = loadUpdateConfig({
    dataDirectory: dataDirectory(),
    executablePath: app.getPath('exe')
  })
  return new ListenerRuntimeService({
    toolsDirectory: join(dataDirectory(), 'tools', 'listener'),
    // Electron's network stack honours the Windows proxy/certificate store;
    // Node/undici fetch does not reliably do so on a desktop installation.
    fetch: (input, init) => net.fetch(input instanceof URL ? input.toString() : input, init),
    repositoryUrl: config.repositoryUrl,
    // Токен только повышает лимит анонимных запросов к GitHub; его отсутствие
    // не ошибка, поэтому неудачный поиск даёт `null`, а не отказ.
    resolveToken: () =>
      resolveGithubToken({ configured: config.githubToken })
        .then((found) => found?.token ?? null)
        .catch(() => null),
    emit: (status) => broadcast({ kind: 'listener-runtime', status }),
    log
  })
}

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
    env: supervisorEnvironment(
      {
        ...process.env,
        // Credentials stored through the settings surface win over anything
        // the ambient environment carries, so the UI is the single source of
        // truth.
        ...(providers?.environment() ?? {})
      },
      coreExecutablePath(),
      dataDirectory()
    )
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
