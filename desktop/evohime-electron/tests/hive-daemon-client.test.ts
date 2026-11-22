import { createServer, type Server, type Socket } from 'node:net'

import { afterEach, describe, expect, it } from 'vitest'

import type { HiveCityMapSnapshot } from '../src/shared/hive-city-map'
import {
  MAX_CITY_MAP_FRAME_BYTES,
  parseHiveCityMapSnapshot
} from '../src/shared/hive-city-map'
import { FrameReader } from '../src/main/ipc/frame-reader'
import {
  createHiveDaemonClient,
  decodeCityMapFrame,
  encodeHiveFrame,
  fetchHiveCityMap,
  HIVE_OBSERVATION_CONTROL_PIPE,
  HIVE_OBSERVATION_CONTROL_PIPE_ENV,
  HIVE_OBSERVATION_CONTROL_SOCKET_ENV,
  hiveControlEndpoint
} from '../src/main/hive-daemon-client'

/**
 * Контракт прямого клиента Electron main к `evohime-hive-daemon`
 * (план 199.3): корректный 4-byte LE framing, жёсткий таймаут, лимит размера
 * фрейма и деградация любого сбоя до типизированного `unavailable`.
 */

let server: Server | null = null

const openSockets = new Set<Socket>()

afterEach(() => {
  for (const socket of openSockets) socket.destroy()
  openSockets.clear()
  // Не ждём `close` — у каждого стаба уникальное имя pipe, гонок не будет.
  server?.close()
  server = null
})

function fixtureIsland(
  id: string,
  role: 'capital' | 'satellite'
): Record<string, unknown> {
  return {
    id,
    name: role === 'capital' ? 'Столица Евы' : 'Спутник',
    role,
    source: 'live_workspace',
    status: 'ready',
    reason: null,
    ecosystem: 'rust',
    structure_hash: 'a'.repeat(16),
    revision_hash: 'b'.repeat(16),
    accent_hue: 212,
    districts: [{ id: 'district-core', label: 'Ядро', kind: 'core' }],
    buildings: [
      {
        id: `${id}/crates/evohime-core`,
        name: 'evohime-core',
        district_id: 'district-core',
        kind: 'citadel',
        relative_path: 'crates/evohime-core',
        file_count: 12,
        line_count: 1200,
        height_weight_bp: 480,
        language: 'rust'
      }
    ],
    edges: [],
    total_files: 12,
    total_lines: 1200
  }
}

function fixtureSnapshotJson(): string {
  const snapshot: HiveCityMapSnapshot = {
    schema_version: 1,
    generated_at_ms: 1_700_000_000_000,
    revision_hash: 'c'.repeat(32),
    capital: fixtureIsland('capital-evohime', 'capital'),
    satellites: [fixtureIsland('sat-abc', 'satellite')]
  } as unknown as HiveCityMapSnapshot
  // Проверяем фикстуру тем же валидатором, что ждёт клиент.
  const parsed = parseHiveCityMapSnapshot(snapshot)
  if (parsed === null) throw new Error('fixture snapshot must satisfy the validator')
  return JSON.stringify(snapshot)
}

let pipeIndex = 0

function uniquePipeName(): string {
  pipeIndex += 1
  return `\\\\.\\pipe\\evohime-test-citymap-${process.pid}-${pipeIndex}`
}

interface StubDaemonOptions {
  /** Отдаётся на каждый полученный запрос; может вернуть буферы для записи. */
  readonly onResponse: (requestPayload: string, socket: Socket) => Buffer[] | void
    // socket нужен вызывающим ответам; часть стабов его не использует.
}

async function startStubDaemon(options: StubDaemonOptions): Promise<string> {
  const pipeName = uniquePipeName()
  const stub = createServer((socket) => {
    openSockets.add(socket)
    socket.on('close', () => openSockets.delete(socket))
    const reader = new FrameReader()
    socket.on('data', (chunk) => {
      const bytes = typeof chunk === 'string' ? Buffer.from(chunk) : chunk
      for (const frame of reader.push(bytes)) {
        const payloadText = Buffer.from(frame).toString('utf8')
        const responses = options.onResponse(payloadText, socket) ?? []
        for (const response of responses) socket.write(response)
      }
    })
    socket.on('error', () => {})
  })
  await new Promise<void>((resolve) => stub.listen(pipeName, resolve))
  server = stub
  return pipeName
}

const EXPECTED_REQUEST = encodeHiveFrame(JSON.stringify({ command: 'get_city_map' }))

describe('hive-daemon-client', () => {
  it('encodeHiveFrame пишет 4-byte LE заголовок и полезную нагрузку', () => {
    const payload = Buffer.from('hello', 'utf8')
    const frame = encodeHiveFrame(payload)
    expect(frame.length).toBe(4 + payload.length)
    expect(frame.readUInt32LE(0)).toBe(payload.length)
    expect(frame.subarray(4)).toEqual(payload)
    expect(() => encodeHiveFrame(Buffer.alloc(MAX_CITY_MAP_FRAME_BYTES + 1))).toThrow(RangeError)
  })

  it('возвращает валидный снапшот по живому демону и проверяет framing запроса', async () => {
    const receivedRequests: string[] = []
    const endpoint = await startStubDaemon({
      onResponse: (requestPayload) => {
        receivedRequests.push(requestPayload)
        return [
          encodeHiveFrame(fixtureSnapshotJson())
        ]
      }
    })

    const outcome = await fetchHiveCityMap({ endpoint, timeoutMs: 1500 })

    expect(outcome.available).toBe(true)
    if (outcome.available) {
      expect(outcome.snapshot.revision_hash).toBe('c'.repeat(32))
      expect(outcome.snapshot.capital.role).toBe('capital')
      expect(outcome.snapshot.satellites).toHaveLength(1)
    }
    // Демон получает ровно один кадр get_city_map в том же 4-byte LE framing.
    expect(receivedRequests).toHaveLength(1)
    const requestPayload = receivedRequests[0] ?? ''
    const expected = JSON.parse(EXPECTED_REQUEST.subarray(4).toString('utf8'))
    expect(JSON.parse(requestPayload)).toEqual(expected)
  })

  it('деградирует до unavailable, когда pipe не существует (ENOENT)', async () => {
    const outcome = await fetchHiveCityMap({
      endpoint: uniquePipeName(),
      timeoutMs: 500
    })
    expect(outcome).toEqual({ available: false, snapshot: null })
  })

  it('деградирует до unavailable по таймауту молчащего демона', async () => {
    const endpoint = await startStubDaemon({ onResponse: () => undefined })
    const outcome = await fetchHiveCityMap({ endpoint, timeoutMs: 120 })
    expect(outcome).toEqual({ available: false, snapshot: null })
  })

  it('отсекает входящий фрейм крупнее MAX_CITY_MAP_FRAME_BYTES', async () => {
    const endpoint = await startStubDaemon({
      onResponse: (_request, socket) => {
        const header = Buffer.alloc(4)
        header.writeUInt32LE(MAX_CITY_MAP_FRAME_BYTES + 1, 0)
        socket.write(header)
        socket.write(Buffer.alloc(1024, 0x61))
      }
    })
    const outcome = await fetchHiveCityMap({ endpoint, timeoutMs: 1500 })
    expect(outcome).toEqual({ available: false, snapshot: null })
  })

  describe('decodeCityMapFrame', () => {
    it('трактует невалидный JSON как unavailable', () => {
      expect(decodeCityMapFrame(Buffer.from('not-json', 'utf8'))).toEqual({
        available: false,
        snapshot: null
      })
    })

    it('трактует не-объект (массив, строка, число) как unavailable', () => {
      for (const raw of ['[]', '42', '"text"', 'null']) {
        expect(decodeCityMapFrame(Buffer.from(raw, 'utf8'))).toEqual({
          available: false,
          snapshot: null
        })
      }
    })

    it('трактует канонический error-фрейм старого демона как unavailable', () => {
      const legacy = JSON.stringify({ error: 'invalid_request: unknown command' })
      expect(decodeCityMapFrame(Buffer.from(legacy, 'utf8'))).toEqual({
        available: false,
        snapshot: null
      })
    })

    it('отклоняет снапшот с неверной схемой или битой структурой', () => {
      const good = JSON.parse(fixtureSnapshotJson())
      const badSchema = { ...good, schema_version: 2 }
      expect(decodeCityMapFrame(Buffer.from(JSON.stringify(badSchema), 'utf8'))).toEqual({
        available: false,
        snapshot: null
      })
      const badIsland = { ...good, capital: { ...good.capital, role: 'satellite' } }
      expect(decodeCityMapFrame(Buffer.from(JSON.stringify(badIsland), 'utf8'))).toEqual({
        available: false,
        snapshot: null
      })
    })
  })

  describe('hiveControlEndpoint', () => {
    it('по умолчанию на Windows использует именованный pipe демона', () => {
      expect(hiveControlEndpoint({}, 'win32')).toBe(HIVE_OBSERVATION_CONTROL_PIPE)
    })

    it('учитывает override EVOHIME_HIVE_CONTROL_PIPE на любой платформе', () => {
      expect(
        hiveControlEndpoint({ [HIVE_OBSERVATION_CONTROL_PIPE_ENV]: 'custom-pipe' }, 'win32')
      ).toBe('custom-pipe')
      expect(
        hiveControlEndpoint({ [HIVE_OBSERVATION_CONTROL_PIPE_ENV]: '  custom-sock  ' }, 'linux')
      ).toBe('custom-sock')
    })

    it('на Unix берёт путь из XDG_RUNTIME_DIR и допускает override сокета', () => {
      expect(
        hiveControlEndpoint({ XDG_RUNTIME_DIR: '/run/user/1000' }, 'linux')
      ).toBe('/run/user/1000/evohime/hive-observation.sock')
      expect(
        hiveControlEndpoint({ HOME: '/home/roman' }, 'linux')
      ).toBe('/home/roman/.cache/evohime/hive-observation.sock')
      expect(
        hiveControlEndpoint(
          {
            XDG_RUNTIME_DIR: '/run/user/1000',
            [HIVE_OBSERVATION_CONTROL_SOCKET_ENV]: '/tmp/hive.sock'
          },
          'linux'
        )
      ).toBe('/tmp/hive.sock')
    })
  })

  it('createHiveDaemonClient отдаёт getCityMap, который всегда резолвится', async () => {
    const endpoint = await startStubDaemon({
      onResponse: () => [encodeHiveFrame(fixtureSnapshotJson())]
    })
    const client = createHiveDaemonClient({ endpoint, timeoutMs: 1500 })
    const outcome = await client.getCityMap()
    expect(outcome.available).toBe(true)
  })
})
