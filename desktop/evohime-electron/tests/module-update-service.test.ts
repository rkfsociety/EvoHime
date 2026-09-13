import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it, vi } from 'vitest'

const { spawnMock } = vi.hoisted(() => ({
  spawnMock: vi.fn(() => ({ once: vi.fn(), unref: vi.fn() }))
}))

vi.mock('node:child_process', () => ({ spawn: spawnMock }))

import { ModuleUpdateService } from '../src/main/update/module-update-service'

describe('ModuleUpdateService', () => {
  it('starts the Windows worker through the shell with quoted paths', async () => {
    const service = new ModuleUpdateService({
      dataDirectory: 'C:\\data\\EvoHime',
      branch: 'main',
      enabled: true,
      updaterPath: 'C:\\Program Files\\EvoHime\\evohime-updater.exe',
      installDirectory: 'C:\\Program Files\\EvoHime',
      emit: () => {},
      intervalMs: 60_000
    })

    await service.prepareComponents(['listener-runtime'])

    const windows = process.platform === 'win32'
    expect(spawnMock).toHaveBeenCalledWith(
      windows ? '"C:\\Program Files\\EvoHime\\evohime-updater.exe"' : 'C:\\Program Files\\EvoHime\\evohime-updater.exe',
      windows ? ['--apply', '--install-dir', '"C:\\Program Files\\EvoHime"'] : ['--apply', '--install-dir', 'C:\\Program Files\\EvoHime'],
      expect.objectContaining({
        detached: true,
        windowsHide: true,
        shell: windows,
        stdio: 'ignore'
      })
    )
  })

  it('marks a crashed worker failed when no status file was written', async () => {
    let close: ((code: number | null, signal: NodeJS.Signals | null) => void) | undefined
    spawnMock.mockImplementationOnce(() => ({
      once: vi.fn((event: string, listener: (code: number | null, signal: NodeJS.Signals | null) => void) => {
        if (event === 'close') close = listener
      }),
      unref: vi.fn()
    }))
    const service = new ModuleUpdateService({
      dataDirectory: 'C:\\missing-update-state',
      branch: 'main',
      enabled: true,
      updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
      installDirectory: 'C:\\EvoHime',
      emit: () => {},
      intervalMs: 60_000
    })

    await service.prepareComponents(['shell-host'])
    close?.(1, null)

    expect(service.status.phase).toBe('failed')
    expect(service.status.error).toContain('без диагностического статуса')
  })

  it('reads the structured worker error and keeps failed status authoritative', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-status-'))
    try {
      const state = join(root, 'update-state')
      mkdirSync(state)
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'failed',
        message: 'Обновление не применено.',
        error: 'updater: transaction worker завершился с кодом 1: invalid shell-host archive',
        available: [{ module: 'shell-host', installed: '0.0.000052', available: '0.0.000053' }]
      }))
      const service = new ModuleUpdateService({
        dataDirectory: root,
        branch: 'main',
        enabled: true,
        updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
        installDirectory: 'C:\\EvoHime',
        emit: () => {},
        intervalMs: 60_000
      })

      await service.runLaunchGate()

      expect(service.status.phase).toBe('failed')
      expect(service.status.message).toBe('Обновление не применено.')
      expect(service.status.error).toContain('invalid shell-host archive')
      service.stop()
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
