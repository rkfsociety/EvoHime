import { randomUUID } from 'node:crypto'

/**
 * Launch contract between the supervisor and the Electron main process.
 *
 * The pipe name, the handshake challenge and the liveness handle are passed by
 * the signed launcher through the process environment. They are never accepted
 * from renderer input or from a command line the renderer can influence
 * (plan 0, stage 1).
 */

export const DEFAULT_CORE_PIPE_NAME = '\\\\.\\pipe\\evohime-core-v1'

export interface LaunchContext {
  /** Full Windows pipe path of the Core endpoint. */
  readonly pipeName: string
  /** Stable identity of this shell instance for the Core handshake. */
  readonly clientId: string
  readonly sessionId: string
  /** One-time supervisor challenge, empty when launched without a supervisor. */
  readonly challenge: string
  /** Name of the OS liveness event owned by the supervisor, when provided. */
  readonly livenessEvent: string
  /** True when no supervisor-provided context was found (developer run). */
  readonly developerLaunch: boolean
}

export function readLaunchContext(
  environment: NodeJS.ProcessEnv = process.env,
  newId: () => string = randomUUID
): LaunchContext {
  const pipeName = sanitize(environment['EVOHIME_CORE_PIPE'])
  const challenge = sanitize(environment['EVOHIME_IPC_CHALLENGE'])
  const livenessEvent = sanitize(environment['EVOHIME_SUPERVISOR_LIVENESS_EVENT'])
  const sessionId = sanitize(environment['EVOHIME_SESSION_ID']) || newId()

  return {
    pipeName: normalizePipeName(pipeName) ?? DEFAULT_CORE_PIPE_NAME,
    clientId: sanitize(environment['EVOHIME_CLIENT_ID']) || `electron-${newId()}`,
    sessionId,
    challenge,
    livenessEvent,
    developerLaunch: pipeName.length === 0 && challenge.length === 0
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

function sanitize(value: string | undefined): string {
  return typeof value === 'string' ? value.trim() : ''
}
