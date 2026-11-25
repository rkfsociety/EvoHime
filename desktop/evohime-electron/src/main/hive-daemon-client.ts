import { connect as netConnect, type Socket } from 'node:net'

import type { HiveCityMapOutcome, HoneycombWorldOutcome } from '@shared/hive-city-map'
// Значения импортируются по относительному пути (в отличие от остального
// src/main): модуль также грузит dev-конфигурация Hive Studio
// (`vite.studio.config.ts`), которую bundler Vite собирает до алиасов.
import {
  MAX_CITY_MAP_FRAME_BYTES,
  parseHiveCityMapSnapshot,
  parseHoneycombWorldState
} from '../shared/hive-city-map'

/**
 * Direct main-process client to `evohime-hive-daemon` over the observation
 * control pipe (plan 199.3). This bypasses Core IPC deliberately: the city
 * map is read-only observation data, and a slow or absent daemon must not
 * touch the Core connection.
 *
 * Reuses the daemon's 4-byte LE length-prefixed JSON framing with a hard
 * timeout. Every failure mode — refused connection, timeout, oversized or
 * malformed frame, invalid JSON, an older daemon's canonical
 * `{"error":"invalid_request: ..."}` answer — degrades to the typed
 * `unavailable` outcome; this function never rejects and never leaks
 * payloads into errors.
 */

/** Same default endpoint as `DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE` in pulse.rs. */
export const HIVE_OBSERVATION_CONTROL_PIPE = String.raw`\\.\pipe\evohime-hive-observation`

/** Same override as `HIVE_OBSERVATION_CONTROL_PIPE_ENV` in pulse.rs. */
export const HIVE_OBSERVATION_CONTROL_PIPE_ENV = 'EVOHIME_HIVE_CONTROL_PIPE'

/** Same override as `HIVE_OBSERVATION_CONTROL_SOCKET_ENV` in pulse.rs. */
export const HIVE_OBSERVATION_CONTROL_SOCKET_ENV = 'EVOHIME_HIVE_CONTROL_SOCKET'

/** Hard response deadline, mirroring the daemon write timeout (1500 ms). */
export const HIVE_CITY_MAP_TIMEOUT_MS = 1500

/** Computes the control endpoint, honoring the daemon's env overrides. */
export function hiveControlEndpoint(
  env: NodeJS.ProcessEnv = process.env,
  platform: NodeJS.Platform = process.platform
): string {
  const pipeOverride = typeof env[HIVE_OBSERVATION_CONTROL_PIPE_ENV] === 'string'
    ? env[HIVE_OBSERVATION_CONTROL_PIPE_ENV]!.trim()
    : ''
  if (pipeOverride.length > 0) return pipeOverride
  if (platform !== 'win32') {
    const socketOverride = typeof env[HIVE_OBSERVATION_CONTROL_SOCKET_ENV] === 'string'
      ? env[HIVE_OBSERVATION_CONTROL_SOCKET_ENV]!.trim()
      : ''
    if (socketOverride.length > 0) return socketOverride
    const runtimeDir =
      typeof env['XDG_RUNTIME_DIR'] === 'string' && env['XDG_RUNTIME_DIR']!.length > 0
        ? env['XDG_RUNTIME_DIR']!
        : `${env['HOME'] ?? ''}/.cache`
    return `${runtimeDir}/evohime/hive-observation.sock`
  }
  return HIVE_OBSERVATION_CONTROL_PIPE
}

/** Encodes one 4-byte LE length-prefixed frame. */
export function encodeHiveFrame(payload: string | Uint8Array): Buffer {
  const bytes = typeof payload === 'string' ? Buffer.from(payload, 'utf8') : Buffer.from(payload)
  if (bytes.length > MAX_CITY_MAP_FRAME_BYTES) {
    throw new RangeError('hive control frame exceeds MAX_CITY_MAP_FRAME_BYTES')
  }
  const frame = Buffer.allocUnsafe(4 + bytes.length)
  frame.writeUInt32LE(bytes.length, 0)
  bytes.copy(frame, 4)
  return frame
}

const GET_CITY_MAP_REQUEST = encodeHiveFrame(
  JSON.stringify({ command: 'get_city_map' })
)

const GET_HONEYCOMB_WORLD_REQUEST = encodeHiveFrame(
  JSON.stringify({ command: 'get_honeycomb_world' })
)

export interface HiveDaemonClientOptions {
  /** Endpoint override; defaults to `hiveControlEndpoint()`. */
  readonly endpoint?: string
  /** Overall response deadline in ms; defaults to 1500. Covers the connect
   * phase too: a named pipe that hangs on open degrades to unavailable. */
  readonly timeoutMs?: number
  /** Connect seam for tests: returns an arbitrary duplex socket. The second
   * argument is the connect-phase deadline in ms. */
  readonly connect?: (endpoint: string, timeoutMs: number) => Promise<Socket>
  /** env seam for tests. */
  readonly env?: NodeJS.ProcessEnv
  /** platform seam for tests. */
  readonly platform?: NodeJS.Platform
}

function defaultConnect(endpoint: string, timeoutMs: number): Promise<Socket> {
  return new Promise((resolve, reject) => {
    const socket = netConnect(endpoint)
    const fail = (error: Error): void => {
      clearTimeout(timer)
      socket.destroy()
      reject(error)
    }
    const pending = setTimeout(() => fail(new Error('connect timeout')), timeoutMs)
    const timer = pending.unref?.() ?? pending
    socket.once('connect', () => {
      clearTimeout(timer)
      socket.off('error', fail)
      resolve(socket)
    })
    socket.once('error', fail)
  })
}

/**
 * Sends `get_city_map` over one fresh connection and validates the answer.
 * Always resolves; never throws.
 */
export async function fetchHiveCityMap(
  options: HiveDaemonClientOptions = {}
): Promise<HiveCityMapOutcome> {
  const bytes = await fetchControlPayload(GET_CITY_MAP_REQUEST, options)
  if (bytes === null) return { available: false, snapshot: null }
  return decodeCityMapFrame(bytes)
}

/**
 * Sends `get_honeycomb_world` over one fresh connection and validates the answer.
 * Always resolves; never throws.
 */
export async function fetchHoneycombWorld(
  options: HiveDaemonClientOptions = {}
): Promise<HoneycombWorldOutcome> {
  const bytes = await fetchControlPayload(GET_HONEYCOMB_WORLD_REQUEST, options)
  if (bytes === null) return { available: false, world: null }
  try {
    const value = JSON.parse(bytes.toString('utf8')) as unknown
    if (value && typeof value === 'object' && !Array.isArray(value) && 'error' in value) {
      return { available: false, world: null }
    }
    const world = parseHoneycombWorldState(value)
    return world === null ? { available: false, world: null } : { available: true, world }
  } catch {
    return { available: false, world: null }
  }
}

async function fetchControlPayload(
  requestFrame: Buffer,
  options: HiveDaemonClientOptions = {}
): Promise<Buffer | null> {
  const timeoutMs = options.timeoutMs ?? HIVE_CITY_MAP_TIMEOUT_MS
  const connect = options.connect ?? defaultConnect
  const endpoint = options.endpoint ?? hiveControlEndpoint(options.env, options.platform)

  let socket: Socket
  try {
    socket = await connect(endpoint, timeoutMs)
  } catch {
    return null
  }

  try {
    return await readResponse(socket, requestFrame, timeoutMs)
  } finally {
    socket.destroy()
  }
}

async function readResponse(
  socket: Socket,
  requestFrame: Buffer,
  timeoutMs: number
): Promise<Buffer | null> {
  return new Promise((resolve) => {
    const chunks: Buffer[] = []
    let received = 0
    let expected: number | null = null
    let settled = false

    const finish = (result: Buffer | null): void => {
      if (settled) return
      settled = true
      socket.off('data', onData)
      socket.off('error', onError)
      socket.off('close', onClose)
      clearTimeout(timer)
      resolve(result)
    }

    const timeout = setTimeout(() => finish(null), timeoutMs)
    const timer = timeout.unref?.() ?? timeout

    const onData = (chunk: Buffer): void => {
      chunks.push(chunk)
      received += chunk.length
      if (received > 4 + MAX_CITY_MAP_FRAME_BYTES) {
        finish(null)
        return
      }
      if (expected === null && received >= 4) {
        const head = Buffer.concat(chunks).subarray(0, 4)
        expected = head.readUInt32LE(0)
        if (expected > MAX_CITY_MAP_FRAME_BYTES) {
          finish(null)
          return
        }
      }
      if (expected !== null && received - 4 >= expected) {
        const buf = Buffer.concat(chunks)
        finish(Buffer.from(buf.subarray(4, 4 + expected)))
      }
    }
    const onError = (): void => finish(null)
    const onClose = (): void => finish(null)

    socket.on('data', onData)
    socket.once('error', onError)
    socket.once('close', onClose)

    socket.write(requestFrame, (error) => {
      if (error) finish(null)
    })
  })
}

/** Splits the wire result into the typed outcome, including the
 * legacy-daemon `{"error": ...}` compatibility branch. */
export function decodeCityMapFrame(payload: Buffer): HiveCityMapOutcome {
  let value: unknown
  try {
    value = JSON.parse(payload.toString('utf8'))
  } catch {
    return { available: false, snapshot: null }
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return { available: false, snapshot: null }
  }
  if ('error' in (value as Record<string, unknown>)) {
    // An old daemon answers any unknown command with this canonical error
    // frame; the shell treats it as "module still catching up".
    return { available: false, snapshot: null }
  }
  const snapshot = parseHiveCityMapSnapshot(value)
  return snapshot === null
    ? { available: false, snapshot: null }
    : { available: true, snapshot }
}

export interface HiveCityMapClient {
  readonly getCityMap: () => Promise<HiveCityMapOutcome>
  readonly getHoneycombWorld?: () => Promise<HoneycombWorldOutcome>
}

/** Builds the long-lived client used by the shell bridge. */
export function createHiveDaemonClient(
  options: HiveDaemonClientOptions = {}
): HiveCityMapClient {
  return {
    getCityMap: () => fetchHiveCityMap(options),
    getHoneycombWorld: () => fetchHoneycombWorld(options)
  }
}
