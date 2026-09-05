import { createHmac, randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

/**
 * Launch contract between the supervisor and the Electron main process.
 *
 * The supervisor writes one protected launch context per session (see
 * `evohime_desktop_ipc::session`) into a directory whose DACL grants only the
 * current user. It holds the unpredictable pipe name and the session secret
 * the shell proves knowledge of during the handshake. The context is never
 * accepted from renderer input or from a command line the renderer can
 * influence.
 */

export const DEFAULT_CORE_PIPE_NAME = '\\\\.\\pipe\\evohime-core-v1'
export const LAUNCH_CONTEXT_FILE = 'session.json'
export const CLIENT_ROLE = 'shell'

const SECRET_HEX_LENGTH = 64

export interface LaunchContext {
  /** Full Windows pipe path of the Core endpoint. */
  readonly pipeName: string
  /** Stable identity of this shell instance for the Core handshake. */
  readonly clientId: string
  readonly sessionId: string
  readonly clientRole: string
  /** Session secret; empty only in an explicitly enabled developer launch. */
  readonly secret: string
  /** Name of the OS liveness event owned by the supervisor, when provided. */
  readonly livenessEvent: string
  readonly supervisorPid?: number
  /** True when no supervisor-provided context was found in developer mode. */
  readonly developerLaunch: boolean
}

/** Answer to Core's single-use nonce: HMAC-SHA256(secret, role|client|nonce). */
export function handshakeProof(context: LaunchContext, nonce: string): string {
  if (context.secret.length === 0) {
    return ''
  }
  return createHmac('sha256', Buffer.from(context.secret, 'utf8'))
    .update(`${context.clientRole}\n${context.clientId}\n${nonce}`, 'utf8')
    .digest('hex')
}

/**
 * Whether a supervisor is known to own the current context.
 *
 * A context file alone proves nothing: Core writes one in its console mode, and
 * older supervisors recorded no pid at all. Such a file outlives the process
 * that made it, so treating its mere presence as "a supervisor is running"
 * leaves the shell reconnecting forever to a pipe nobody serves. Only a
 * recorded, still-alive pid counts; anything else means start a supervisor.
 */
export function hasLiveSupervisor(
  context: LaunchContext,
  isAlive: (pid: number) => boolean
): boolean {
  if (context.developerLaunch || context.supervisorPid === undefined) {
    return false
  }
  return isAlive(context.supervisorPid)
}

export function launchContextPath(environment: NodeJS.ProcessEnv = process.env): string {
  const explicit = sanitize(environment['EVOHIME_LAUNCH_CONTEXT'])
  if (explicit) {
    return explicit
  }
  const dataDir =
    sanitize(environment['EVOHIME_DATA_DIR']) ||
    join(sanitize(environment['LOCALAPPDATA']) || '', 'EvoHime')
  return join(dataDir, 'runtime', LAUNCH_CONTEXT_FILE)
}

/**
 * Reads the current launch context. It is re-read before every connection
 * attempt so a supervisor that rotated the session is picked up without
 * restarting the shell.
 */
export function readLaunchContext(
  environment: NodeJS.ProcessEnv = process.env,
  newId: () => string = randomUUID,
  readFile: (path: string) => string = (path) => readFileSync(path, 'utf8')
): LaunchContext {
  const identity = {
    clientId: sanitize(environment['EVOHIME_CLIENT_ID']) || `electron-${newId()}`,
    sessionId: sanitize(environment['EVOHIME_SESSION_ID']) || newId(),
    clientRole: CLIENT_ROLE,
    livenessEvent: sanitize(environment['EVOHIME_SUPERVISOR_LIVENESS_EVENT'])
  }

  const file = parseContextFile(readSafely(readFile, launchContextPath(environment)))
  if (file) {
    const context = {
      ...identity,
      pipeName: file.pipeName,
      secret: file.secret,
      livenessEvent: file.livenessEvent,
      developerLaunch: false
    }
    return file.supervisorPid === undefined ? context : { ...context, supervisorPid: file.supervisorPid }
  }

  return {
    ...identity,
    pipeName: normalizePipeName(sanitize(environment['EVOHIME_CORE_PIPE'])) ?? DEFAULT_CORE_PIPE_NAME,
    secret: '',
    developerLaunch: true
  }
}

/**
 * Accepts only a local named pipe path. A remote (`\\host\pipe\…`) or
 * otherwise malformed value is rejected instead of being connected to.
 */
export function normalizePipeName(value: string): string | null {
  if (value.length === 0 || value.length > 256) {
    return null
  }
  const name = value.startsWith('\\\\.\\pipe\\') ? value.slice('\\\\.\\pipe\\'.length) : value
  if (name.length === 0 || !/^[A-Za-z0-9._-]+$/.test(name)) {
    return null
  }
  return `\\\\.\\pipe\\${name}`
}

function parseContextFile(raw: string | null): { pipeName: string; secret: string; supervisorPid?: number; livenessEvent: string } | null {
  if (raw === null) {
    return null
  }
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return null
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return null
  }
  const record = parsed as Record<string, unknown>
  const pipeName = normalizePipeName(
    typeof record['pipe_name'] === 'string' ? record['pipe_name'] : ''
  )
  const secret = typeof record['secret'] === 'string' ? record['secret'] : ''
  const supervisorPid = typeof record['supervisor_pid'] === 'number' && Number.isInteger(record['supervisor_pid']) && record['supervisor_pid'] > 0
    ? record['supervisor_pid']
    : undefined
  const livenessEvent = typeof record['supervisor_liveness_event'] === 'string'
    ? record['supervisor_liveness_event'].trim()
    : ''
  if (livenessEvent.length > 256 || (livenessEvent !== '' && !/^Local\\EvoHime\.Supervisor\.Liveness\.[A-Za-z0-9]+$/.test(livenessEvent))) {
    return null
  }
  if (pipeName === null || !/^[0-9a-f]+$/.test(secret) || secret.length !== SECRET_HEX_LENGTH) {
    return null
  }
  return supervisorPid === undefined
    ? { pipeName, secret, livenessEvent }
    : { pipeName, secret, supervisorPid, livenessEvent }
}

function readSafely(readFile: (path: string) => string, path: string): string | null {
  try {
    return readFile(path)
  } catch {
    return null
  }
}

function sanitize(value: string | undefined): string {
  return typeof value === 'string' ? value.trim() : ''
}
